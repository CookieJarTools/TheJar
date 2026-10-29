use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::Installed;
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
    let record = load_manifest().apps.remove(&app.id)?;
    if !record.file.is_file() {
        return None;
    }
    Some(Installed {
        version: Version::parse(&record.version),
        version_text: record.version,
        location: record.file.parent()?.to_path_buf(),
        program: record.file,
        machine_wide: false,
    })
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
    _existing: Option<&Installed>,
) -> Result<(), String> {
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Couldn't make {} runnable: {e}", file.display()))?;

    let icon_path = paths::data_dir().join("icons").join(format!("{}.png", app.id));
    if let Some(bytes) = icon {
        paths::write_atomic(&icon_path, bytes).map_err(|e| format!("Couldn't save the icon: {e}"))?;
    }
    let icon_path = icon_path.is_file().then_some(icon_path);
    let desktop_file = write_menu_entry(app, file, icon_path.as_deref())?;

    remove_other_versions(file);
    update_manifest(|manifest| {
        manifest.apps.insert(
            app.id.clone(),
            Record { version: release.version.to_string(), file: file.to_path_buf(), desktop_file, icon: icon_path },
        );
    })
}

pub fn uninstall(app: &AppSpec, installed: &Installed) -> Result<(), String> {
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
}
