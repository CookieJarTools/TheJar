use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::{Installed, Origin};
use crate::catalog::AppSpec;
use crate::github::Release;
use crate::http::Asset;
use crate::paths;
use crate::version::Version;

/// AppImages can be swapped while they run; the running copy keeps the old file open.
pub const CLOSES_APP_TO_UPDATE: bool = false;

/// Marks menu entries The Jar wrote, so it never overwrites or deletes one it didn't.
const MARKER: &str = "X-TheJar-App=";
const LIBFUSE2: [&str; 6] = [
    "/usr/lib/libfuse.so.2",
    "/usr/lib64/libfuse.so.2",
    "/lib64/libfuse.so.2",
    "/usr/lib/x86_64-linux-gnu/libfuse.so.2",
    "/lib/x86_64-linux-gnu/libfuse.so.2",
    "/usr/lib/aarch64-linux-gnu/libfuse.so.2",
];
/// Set by the AppImage runtime for The Jar itself; a child AppImage must not inherit them.
const APPIMAGE_VARS: [&str; 4] = ["APPIMAGE", "APPDIR", "OWD", "ARGV0"];

static MANIFEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    apps: BTreeMap<String, Record>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    version: String,
    file: PathBuf,
    #[serde(default)]
    desktop_file: Option<PathBuf>,
    #[serde(default)]
    icon: Option<PathBuf>,
}

fn manifest_path() -> PathBuf {
    paths::data_dir().join("installed.json")
}

fn load_manifest() -> Manifest {
    std::fs::read_to_string(manifest_path()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn update_manifest(change: impl FnOnce(&mut Manifest)) -> Result<(), String> {
    let _guard = MANIFEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut manifest = load_manifest();
    change(&mut manifest);
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    paths::write_atomic(&manifest_path(), text.as_bytes())
        .map_err(|e| format!("Couldn't save the list of installed apps: {e}"))
}

pub fn detect(app: &AppSpec) -> Option<Installed> {
    let Some(record) = load_manifest().apps.remove(&app.id).filter(|record| record.file.is_file()) else {
        return find_elsewhere(app);
    };
    // A copy The Jar found and updated stays where it was found.
    let origin = if record.file.starts_with(paths::apps_dir()) { Origin::Jar } else { Origin::Found };
    Some(Installed {
        version: Version::parse(&record.version),
        version_text: record.version,
        location: record.file.parent()?.to_path_buf(),
        program: record.file,
        machine_wide: false,
        origin,
    })
}

/// A copy The Jar didn't install: one that a menu entry named after the app opens, or an
/// AppImage in ~/Applications or ~/AppImages.
fn find_elsewhere(app: &AppSpec) -> Option<Installed> {
    let (program, entry_version) = menu_entries(app)
        .into_iter()
        .find_map(|(_, entry)| Some((entry_program(&entry)?, desktop_value(&entry, "X-AppImage-Version"))))
        .or_else(|| loose_appimage(app).map(|program| (program, None)))?;
    let in_home = dirs::home_dir().is_some_and(|home| program.starts_with(home));
    let package = if in_home { None } else { package_owner(&program) };
    let version_text = match &package {
        Some((_, version)) => version.clone(),
        None => entry_version.or_else(|| version_in_name(&program)).unwrap_or_default(),
    };
    let origin = match package {
        _ if in_home => Origin::Found,
        Some((manager, _)) => Origin::System(Some(manager)),
        None => Origin::System(None),
    };
    Some(Installed {
        version: Version::parse(&version_text),
        version_text,
        location: program.parent()?.to_path_buf(),
        program,
        machine_wide: false,
        origin,
    })
}

/// Menu entries The Jar didn't write that are named after the app, the user's own first.
fn menu_entries(app: &AppSpec) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for dir in application_dirs() {
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut candidates: Vec<PathBuf> = files
            .flatten()
            .map(|file| file.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
            .filter(|path| path.file_stem().is_some_and(|stem| names_app(&stem.to_string_lossy(), app)))
            .collect();
        candidates.sort();
        for path in candidates {
            let Ok(entry) = std::fs::read_to_string(&path) else {
                continue;
            };
            let hidden = desktop_value(&entry, "Hidden").is_some_and(|value| value == "true");
            if !hidden && !entry.lines().any(|line| line.starts_with(MARKER)) {
                found.push((path, entry));
            }
        }
    }
    found
}

/// `~/.local/share/applications`, then the system's, from `XDG_DATA_DIRS`.
fn application_dirs() -> Vec<PathBuf> {
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    let mut dirs = vec![paths::applications_dir()];
    for dir in system.split(':').filter(|dir| dir.starts_with('/')) {
        let dir = Path::new(dir).join("applications");
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// `whisk.desktop`, `Whisk.desktop`, or AppImageLauncher's `appimagekit_<hash>-Whisk.desktop`.
fn names_app(stem: &str, app: &AppSpec) -> bool {
    let stem = stem.to_lowercase();
    let stem = match stem.strip_prefix("appimagekit_") {
        Some(rest) => rest.split_once('-').map_or(rest, |(_, name)| name),
        None => &stem,
    };
    [app.desktop_name(), &app.name, &app.id].iter().any(|name| stem == name.to_lowercase())
}

/// The value of `key` in a desktop entry's main group.
fn desktop_value(entry: &str, key: &str) -> Option<String> {
    let mut in_main = false;
    for line in entry.lines().map(str::trim) {
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
        } else if in_main
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim().to_owned());
        }
    }
    None
}

/// The file a menu entry's `Exec=` line runs, with any symlinks followed, if it exists.
fn entry_program(entry: &str) -> Option<PathBuf> {
    let program = exec_target(entry)?;
    let path = if program.starts_with('/') {
        PathBuf::from(program)
    } else if program.contains('/') {
        return None;
    } else {
        std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(&program)).find(|path| path.is_file())?
    };
    std::fs::canonicalize(path).ok().filter(|path| path.is_file())
}

/// The program an `Exec=` line names, unescaped: a path, or a command to look for in PATH.
fn exec_target(entry: &str) -> Option<String> {
    let exec = desktop_value(entry, "Exec")?.replace("\\\\", "\\");
    let mut words = exec_words(&exec).into_iter();
    let mut program = words.next()?;
    if program == "env" || program.ends_with("/env") {
        program = words.find(|word| !word.contains('='))?;
    }
    Some(program.replace("%%", "%"))
}

/// Splits an `Exec=` value into words. Inside double quotes a backslash escapes the next character.
fn exec_words(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut started) = (false, false);
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => word.extend(chars.next()),
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// An AppImage named after the app in ~/Applications or ~/AppImages, the newest if there are several.
fn loose_appimage(app: &AppSpec) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let names = [app.name.to_lowercase(), app.id.to_lowercase()];
    ["Applications", "AppImages"]
        .iter()
        .filter_map(|dir| std::fs::read_dir(home.join(dir)).ok())
        .flatten()
        .flatten()
        .map(|file| file.path())
        .filter(|path| {
            let file = path.file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default();
            file.ends_with(".appimage")
                && names.iter().any(|name| {
                    file.strip_prefix(name.as_str()).is_some_and(|rest| rest.starts_with(['-', '_', '.', ' ']))
                })
        })
        .filter_map(|path| std::fs::canonicalize(path).ok().filter(|path| path.is_file()))
        .max_by_key(|path| version_in_name(path).as_deref().and_then(Version::parse))
}

/// `Whisk_0.3.0_amd64.AppImage` → `0.3.0`.
fn version_in_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let stem = name.strip_suffix(".AppImage").or_else(|| name.strip_suffix(".appimage")).unwrap_or(&name);
    stem.split(['-', '_', ' '])
        .map(|part| part.strip_prefix('v').unwrap_or(part))
        .find(|part| {
            part.starts_with(|c: char| c.is_ascii_digit()) && part.contains('.') && Version::parse(part).is_some()
        })
        .map(str::to_owned)
}

/// The package manager that owns `path`, with the package's version.
fn package_owner(path: &Path) -> Option<(&'static str, String)> {
    let path = path.to_str()?;
    if let Some(owner) = output("pacman", &["-Qo", path]) {
        // "/usr/bin/whisk is owned by whisk 0.3.0-1"
        return Some(("pacman", upstream(owner.split_whitespace().last()?)));
    }
    if let Some(owner) = output("dpkg-query", &["-S", path]) {
        // "whisk: /usr/bin/whisk"
        let package = owner.lines().next()?.rsplit_once(": ")?.0.to_owned();
        let version = output("dpkg-query", &["-W", "-f=${Version}", &package])?;
        return Some(("dpkg", upstream(&version)));
    }
    output("rpm", &["-qf", "--qf", "%{VERSION}", path]).map(|version| ("rpm", version))
}

/// `1:0.3.0-2` → `0.3.0`: drops the epoch and the packaging revision.
fn upstream(version: &str) -> String {
    let version = version.split_once(':').map_or(version, |(_, rest)| rest);
    version.rsplit_once('-').map_or(version, |(upstream, _)| upstream).to_owned()
}

/// What a command prints, if it succeeds and prints anything.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out =
        Command::new(program).args(args).env("LC_ALL", "C").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

pub fn download_path(app: &AppSpec, asset: &Asset) -> Result<PathBuf, String> {
    let name = Path::new(&asset.name)
        .file_name()
        .filter(|name| !name.is_empty() && *name != "..")
        .ok_or_else(|| format!("The release file name {:?} can't be used.", asset.name))?;
    Ok(paths::apps_dir().join(&app.id).join(name))
}

pub fn install(
    app: &AppSpec,
    release: &Release,
    file: &Path,
    icon: Option<&[u8]>,
    existing: Option<&Installed>,
) -> Result<(), String> {
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Couldn't make {} runnable: {e}", file.display()))?;
    if let Some(existing) = existing.filter(|existing| existing.origin == Origin::Found) {
        return replace_found(app, release, file, icon, existing);
    }

    let icon_path = save_icon(app, icon)?;
    let desktop_file = write_menu_entry(app, file, icon_path.as_deref())?;

    remove_other_versions(file);
    update_manifest(|manifest| {
        manifest.apps.insert(
            app.id.clone(),
            Record { version: release.version.to_string(), file: file.to_path_buf(), desktop_file, icon: icon_path },
        );
    })
}

/// Updates a copy The Jar found by putting the new file where the old one was, so whatever
/// opens it (a menu entry, a login service, a command in PATH) carries on working.
fn replace_found(
    app: &AppSpec,
    release: &Release,
    file: &Path,
    icon: Option<&[u8]>,
    existing: &Installed,
) -> Result<(), String> {
    let old = &existing.program;
    let opened = menu_entries(app).iter().any(|(_, entry)| entry_program(entry).as_ref() == Some(old));
    // Nothing else points at a loose AppImage, so one named after its version can take the new
    // release's name, as The Jar's own copies do.
    let renamed = (!opened && version_in_name(old).is_some())
        .then(|| old.parent().zip(file.file_name()).map(|(folder, name)| folder.join(name)))
        .flatten();
    let target = renamed.as_ref().unwrap_or(old);
    let moved = move_file(file, target);
    if let Some(folder) = file.parent() {
        let _ = std::fs::remove_dir(folder);
    }
    moved.map_err(|e| format!("Couldn't replace {}: {e}", paths::pretty(old)))?;
    if target != old {
        let _ = std::fs::remove_file(old);
    }

    // A loose AppImage that nothing opens gets a menu entry of The Jar's.
    let (desktop_file, icon_path) = if opened {
        (None, None)
    } else {
        let icon_path = save_icon(app, icon)?;
        (write_menu_entry(app, target, icon_path.as_deref())?, icon_path)
    };
    update_manifest(|manifest| {
        manifest.apps.insert(
            app.id.clone(),
            Record { version: release.version.to_string(), file: target.clone(), desktop_file, icon: icon_path },
        );
    })
}

/// Renames `from` onto `to`, copying through a file beside `to` when they're on different drives.
/// `from` is gone afterwards either way.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let mut temp = to.as_os_str().to_owned();
    temp.push(".thejar-new");
    let temp = PathBuf::from(temp);
    let result = std::fs::copy(from, &temp)
        .and_then(|_| std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755)))
        .and_then(|_| std::fs::rename(&temp, to));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    let _ = std::fs::remove_file(from);
    result
}

fn save_icon(app: &AppSpec, icon: Option<&[u8]>) -> Result<Option<PathBuf>, String> {
    let icon_path = paths::data_dir().join("icons").join(format!("{}.png", app.id));
    if let Some(bytes) = icon {
        paths::write_atomic(&icon_path, bytes).map_err(|e| format!("Couldn't save the icon: {e}"))?;
    }
    Ok(icon_path.is_file().then_some(icon_path))
}

pub fn uninstall(app: &AppSpec, installed: &Installed) -> Result<(), String> {
    if installed.origin == Origin::Found {
        remove_found(app, installed)?;
    }
    let record = load_manifest().apps.remove(&app.id);
    let folder = paths::apps_dir().join(&app.id);
    // Only ever delete inside The Jar's own folder, whatever the manifest says.
    if installed.location.starts_with(paths::apps_dir()) && folder.is_dir() {
        std::fs::remove_dir_all(&folder).map_err(|e| format!("Couldn't delete {}: {e}", folder.display()))?;
    }
    if let Some(record) = &record {
        if let Some(desktop) = &record.desktop_file
            && is_ours(desktop)
        {
            let _ = std::fs::remove_file(desktop);
        }
        if let Some(icon) = &record.icon {
            let _ = std::fs::remove_file(icon);
        }
    }
    refresh_menu_cache();
    update_manifest(|manifest| {
        manifest.apps.remove(&app.id);
    })
}

/// Deletes a copy The Jar found, and the menu entries in the home folder that open it.
fn remove_found(app: &AppSpec, installed: &Installed) -> Result<(), String> {
    // Only ever delete inside the home folder.
    let Some(home) = dirs::home_dir().filter(|home| installed.program.starts_with(home)) else {
        return Err(format!("The Jar won't delete {}.", installed.program.display()));
    };
    // Work out which entries open it before the file goes, since that's how they're matched.
    let entries: Vec<PathBuf> = menu_entries(app)
        .into_iter()
        .filter(|(path, entry)| path.starts_with(&home) && entry_program(entry).as_ref() == Some(&installed.program))
        .map(|(path, _)| path)
        .collect();
    if installed.program.is_file() {
        std::fs::remove_file(&installed.program)
            .map_err(|e| format!("Couldn't delete {}: {e}", paths::pretty(&installed.program)))?;
    }
    for entry in entries {
        let _ = std::fs::remove_file(entry);
    }
    Ok(())
}

pub fn launch(installed: &Installed) -> Result<(), String> {
    let mut command = Command::new(&installed.program);
    for var in APPIMAGE_VARS {
        command.env_remove(var);
    }
    if !LIBFUSE2.iter().any(|lib| Path::new(lib).exists()) {
        // Older AppImages need libfuse2 to mount themselves; without it they can unpack instead.
        command.env("APPIMAGE_EXTRACT_AND_RUN", "1");
    }
    if let Some(home) = dirs::home_dir() {
        command.current_dir(home);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("Couldn't open {}: {e}", installed.program.display()))?;
    // Reap it when it exits so it doesn't linger as a zombie while The Jar is open.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The AppImage runtime process keeps the AppImage file as its executable while the app runs.
pub fn is_running(_app: &AppSpec, installed: &Installed) -> bool {
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return false;
    };
    let target = installed.program.as_os_str().as_encoded_bytes();
    processes.flatten().any(|entry| {
        let Ok(exe) = std::fs::read_link(entry.path().join("exe")) else {
            return false;
        };
        let exe = exe.as_os_str().as_encoded_bytes();
        exe == target || exe.strip_suffix(b" (deleted)") == Some(target)
    })
}

/// Gives The Jar's own AppImage a menu entry pointing at wherever it is now.
pub fn integrate_self(app: &AppSpec, appimage: &Path, icon: &[u8]) {
    let icon_path = paths::data_dir().join("icons").join(format!("{}.png", app.id));
    if std::fs::read(&icon_path).ok().as_deref() != Some(icon) {
        let _ = paths::write_atomic(&icon_path, icon);
    }
    let path = paths::applications_dir().join(format!("{}.desktop", app.desktop_name()));
    let entry = desktop_entry(app, appimage, Some(&icon_path), false);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(entry.as_str()) {
        return;
    }
    if !path.exists() || is_ours(&path) {
        let _ = paths::write_atomic(&path, entry.as_bytes());
    }
}

fn write_menu_entry(app: &AppSpec, program: &Path, icon: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let dir = paths::applications_dir();
    let mut path = dir.join(format!("{}.desktop", app.desktop_name()));
    if path.exists() && !is_ours(&path) {
        // Someone else's entry has that name; leave it alone and use one of our own.
        path = dir.join(format!("thejar-{}.desktop", app.id));
    }
    let entry = desktop_entry(app, program, icon, true);
    paths::write_atomic(&path, entry.as_bytes()).map_err(|e| format!("Couldn't add the menu entry: {e}"))?;

    refresh_menu_cache();
    let desktop_id = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !app.linux.protocols.is_empty()
        && let Some(config) = dirs::config_dir()
    {
        // xdg-mime gives up if mimeapps.list's folder doesn't exist yet.
        let _ = std::fs::create_dir_all(config);
    }
    for scheme in &app.linux.protocols {
        run_quietly("xdg-mime", &["default", &desktop_id, &format!("x-scheme-handler/{scheme}")]);
    }
    Ok(Some(path))
}

fn refresh_menu_cache() {
    let dir = paths::applications_dir();
    run_quietly("update-desktop-database", &["-q", &dir.to_string_lossy()]);
}

fn run_quietly(program: &str, args: &[&str]) {
    let _ = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

fn is_ours(desktop_file: &Path) -> bool {
    std::fs::read_to_string(desktop_file).is_ok_and(|text| text.lines().any(|line| line.starts_with(MARKER)))
}

fn remove_other_versions(keep: &Path) {
    let Some(folder) = keep.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("appimage"));
        if stale && path != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn desktop_entry(app: &AppSpec, program: &Path, icon: Option<&Path>, takes_urls: bool) -> String {
    let mut lines =
        vec!["[Desktop Entry]".to_owned(), "Type=Application".to_owned(), format!("Name={}", entry_value(&app.name))];
    if !app.tagline.is_empty() {
        lines.push(format!("Comment={}", entry_value(&app.tagline)));
    }
    let args = if takes_urls { " %U" } else { "" };
    lines.push(format!("Exec={}{args}", exec_quote(program)));
    if let Some(icon) = icon {
        lines.push(format!("Icon={}", entry_value(&icon.to_string_lossy())));
    }
    lines.push("Terminal=false".to_owned());
    let categories = app.linux.categories.as_deref().unwrap_or("Utility;");
    let categories = categories.trim_end_matches(';');
    lines.push(format!("Categories={};", entry_value(categories)));
    lines.push(format!("StartupWMClass={}", app.desktop_name()));
    if !app.linux.protocols.is_empty() {
        let types: String = app.linux.protocols.iter().map(|scheme| format!("x-scheme-handler/{scheme};")).collect();
        lines.push(format!("MimeType={types}"));
    }
    lines.push(format!("{MARKER}{}", app.id));
    lines.join("\n") + "\n"
}

/// A desktop entry string value: one line, backslashes escaped.
fn entry_value(text: &str) -> String {
    text.replace('\\', "\\\\").replace(['\n', '\r', '\t'], " ")
}

/// Quotes a path for `Exec=`. Inside quotes, `"`, `` ` ``, `$` and `\` take a backslash; then
/// the whole value is escaped as a string, which doubles every backslash again.
fn exec_quote(path: &Path) -> String {
    let mut quoted = String::from("\"");
    for c in path.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            '\n' | '\r' => quoted.push(' '),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn writes_a_menu_entry_that_handles_links() {
        let apps = catalog::parse(include_str!("../../apps.json")).unwrap();
        let entry = desktop_entry(
            &apps[0],
            Path::new("/home/jo/.local/share/the-jar/apps/crumbtree/CrumbTree-0.2.0.AppImage"),
            Some(Path::new("/home/jo/.local/share/the-jar/icons/crumbtree.png")),
            true,
        );
        assert!(entry.starts_with("[Desktop Entry]\nType=Application\nName=CrumbTree\n"));
        assert!(
            entry.contains("\nExec=\"/home/jo/.local/share/the-jar/apps/crumbtree/CrumbTree-0.2.0.AppImage\" %U\n")
        );
        assert!(entry.contains("\nCategories=Development;RevisionControl;\n"));
        assert!(entry.contains("\nStartupWMClass=crumbtree\n"));
        assert!(entry.contains("\nMimeType=x-scheme-handler/crumbtree;\n"));
        assert!(entry.ends_with("\nX-TheJar-App=crumbtree\n"));
    }

    #[test]
    fn quotes_awkward_paths() {
        assert_eq!(exec_quote(Path::new("/a b/c")), "\"/a b/c\"");
        assert_eq!(exec_quote(Path::new("/a$b/100%")), "\"/a\\\\$b/100%%\"");
        assert_eq!(exec_quote(Path::new("/a\"b")), "\"/a\\\\\"b\"");
        assert_eq!(exec_quote(Path::new("/a\\b")), "\"/a\\\\\\\\b\"");
    }

    #[test]
    fn reads_the_program_from_exec_lines() {
        let path = "/home/jo/My Apps/$odd \"one\"/100%/Whisk.AppImage";
        let ours = format!("[Desktop Entry]\nExec={} %U\n", exec_quote(Path::new(path)));
        assert_eq!(exec_target(&ours).as_deref(), Some(path));
        let postie = "[Desktop Entry]\nName=Postie\nExec=/home/jo/.local/bin/postie %u\n";
        assert_eq!(exec_target(postie).as_deref(), Some("/home/jo/.local/bin/postie"));
        let wrapped = "[Desktop Entry]\nExec=env GDK_BACKEND=x11 whisk %F\n";
        assert_eq!(exec_target(wrapped).as_deref(), Some("whisk"));
        // Only the main group counts.
        let action = "[Desktop Entry]\nName=Postie\n\n[Desktop Action compose]\nExec=postie compose\n";
        assert_eq!(exec_target(action), None);
    }

    #[test]
    fn recognises_menu_entries_named_after_the_app() {
        let apps = catalog::parse(include_str!("../../apps.json")).unwrap();
        let whisk = apps.iter().find(|app| app.id == "whisk").unwrap();
        for stem in ["whisk", "Whisk", "appimagekit_0f3c1a9e-Whisk"] {
            assert!(names_app(stem, whisk), "{stem}");
        }
        for stem in ["whisker", "xfce4-whiskermenu", "appimagekit_0f3c1a9e-Whisker", "thejar-whisk"] {
            assert!(!names_app(stem, whisk), "{stem}");
        }
    }

    #[test]
    fn finds_versions_in_names() {
        let version = |name: &str| version_in_name(Path::new(name));
        assert_eq!(version("/a/Whisk_0.3.0_amd64.AppImage").as_deref(), Some("0.3.0"));
        assert_eq!(version("CrumbTree-0.2.0.AppImage").as_deref(), Some("0.2.0"));
        assert_eq!(version("GameBridge-v1.4.2-x86_64.AppImage").as_deref(), Some("1.4.2"));
        assert_eq!(version("whisk-1.2"), Some("1.2".to_owned()));
        assert_eq!(version("Whisk-x86_64.AppImage"), None);
        assert_eq!(version("/home/jo/.local/bin/postie"), None);

        assert_eq!(upstream("1:0.3.0-2"), "0.3.0");
        assert_eq!(upstream("0.4.0-beta.1-1"), "0.4.0-beta.1");
        assert_eq!(upstream("0.3.0"), "0.3.0");
    }
}
