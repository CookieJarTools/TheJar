use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

use super::{Installed, Origin, split_command};
use crate::catalog::AppSpec;
use crate::github::Release;
use crate::http::Asset;
use crate::paths;
use crate::version::Version;

/// The NSIS installers close a running copy before replacing its files.
pub const CLOSES_APP_TO_UPDATE: bool = true;

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Entry {
    display_name: String,
    version: String,
    location: Option<PathBuf>,
    icon: Option<PathBuf>,
    uninstall: Option<String>,
    quiet_uninstall: Option<String>,
    machine_wide: bool,
}

/// Apps & Features entries from the per-user and per-machine registry.
fn entries() -> Vec<Entry> {
    let mut found = Vec::new();
    for (hive, machine_wide) in [(HKEY_CURRENT_USER, false), (HKEY_LOCAL_MACHINE, true)] {
        let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(UNINSTALL_KEY, KEY_READ | KEY_WOW64_64KEY) else {
            continue;
        };
        for name in key.enum_keys().flatten() {
            let Ok(sub) = key.open_subkey_with_flags(&name, KEY_READ | KEY_WOW64_64KEY) else {
                continue;
            };
            let read = |value: &str| sub.get_value::<String, _>(value).ok().filter(|v| !v.trim().is_empty());
            let Some(display_name) = read("DisplayName") else {
                continue;
            };
            found.push(Entry {
                display_name,
                version: read("DisplayVersion").unwrap_or_default(),
                location: read("InstallLocation").map(|p| PathBuf::from(p.trim_matches('"'))),
                icon: read("DisplayIcon").map(|icon| icon_path(&icon)),
                uninstall: read("UninstallString"),
                quiet_uninstall: read("QuietUninstallString"),
                machine_wide,
            });
        }
    }
    found
}

/// `"C:\App\App.exe",0` → `C:\App\App.exe`
fn icon_path(icon: &str) -> PathBuf {
    let icon = icon.trim();
    let icon = match icon.rsplit_once(',') {
        Some((path, index)) if index.trim().parse::<i32>().is_ok() => path,
        _ => icon,
    };
    PathBuf::from(icon.trim().trim_matches('"'))
}

/// electron-builder registers "CrumbTree 0.2.0", or just "GameBridge" with a custom display name.
fn is_named(display_name: &str, product: &str) -> bool {
    display_name == product
        || display_name
            .strip_prefix(product)
            .and_then(|rest| rest.strip_prefix(' '))
            .is_some_and(|rest| Version::parse(rest).is_some())
}

fn find(app: &AppSpec) -> Option<Entry> {
    entries().into_iter().find(|e| is_named(&e.display_name, app.product_name()))
}

pub fn detect(app: &AppSpec) -> Option<Installed> {
    let entry = find(app)?;
    let location =
        entry.location.clone().or_else(|| entry.icon.as_ref().and_then(|icon| icon.parent().map(Path::to_path_buf)))?;
    let exe = location.join(app.exe_name());
    let program = if exe.is_file() { exe } else { entry.icon.clone().unwrap_or(exe) };
    Some(Installed {
        version: Version::parse(&entry.version),
        version_text: entry.version,
        location,
        program,
        machine_wide: entry.machine_wide,
        origin: Origin::Jar,
    })
}

pub fn download_path(_app: &AppSpec, asset: &Asset) -> Result<PathBuf, String> {
    let name = Path::new(&asset.name)
        .file_name()
        .filter(|name| !name.is_empty() && *name != "..")
        .ok_or_else(|| format!("The release file name {:?} can't be used.", asset.name))?;
    Ok(paths::cache_dir().join("downloads").join(name))
}

pub fn install(
    _app: &AppSpec,
    _release: &Release,
    file: &Path,
    _icon: Option<&[u8]>,
    existing: Option<&Installed>,
) -> Result<(), String> {
    let mut command = Command::new(file);
    command.raw_arg("/S");
    let machine_wide = existing.is_some_and(|e| e.machine_wide);
    command.raw_arg(if machine_wide { "/allusers" } else { "/currentuser" });
    if let Some(existing) = existing {
        // NSIS wants /D last and unquoted, even with spaces in the path.
        command.raw_arg(format!("/D={}", existing.location.display()));
    }
    let status = command.status().map_err(|e| match e.raw_os_error() {
        Some(740) => "This app was installed for everyone on this PC, so updating it needs admin rights. Run its installer yourself.".to_owned(),
        _ => format!("Couldn't start the installer: {e}"),
    });
    let _ = std::fs::remove_file(file);
    let status = status?;
    if status.success() {
        return Ok(());
    }
    Err(match status.code() {
        Some(1) | Some(2) => "The installer was cancelled or couldn't finish.".to_owned(),
        Some(code) => format!("The installer stopped with code {code}."),
        None => "The installer stopped unexpectedly.".to_owned(),
    })
}

pub fn uninstall(app: &AppSpec, _installed: &Installed) -> Result<(), String> {
    let entry = find(app).ok_or("It's already gone.")?;
    let command = match (entry.quiet_uninstall, entry.uninstall) {
        (Some(quiet), _) => quiet,
        (None, Some(plain)) => format!("{plain} /S"),
        (None, None) => return Err("Windows doesn't list an uninstaller for it.".to_owned()),
    };
    let (program, args) = split_command(&command).ok_or("Windows lists an uninstaller The Jar couldn't read.")?;
    Command::new(program).raw_arg(args).status().map_err(|e| format!("Couldn't start the uninstaller: {e}"))?;

    // NSIS uninstallers copy themselves to a temp folder and carry on from there, so the
    // process above exits early. Wait for the Apps & Features entry to go instead.
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(90) {
        if find(app).is_none() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err("The uninstaller didn't finish. It may be waiting for you to close the app.".to_owned())
}

pub fn launch(installed: &Installed) -> Result<(), String> {
    Command::new(&installed.program)
        .current_dir(&installed.location)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Couldn't open {}: {e}", installed.program.display()))
}

pub fn is_running(_app: &AppSpec, installed: &Installed) -> bool {
    let Some(exe) = installed.program.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return false;
    };
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {exe}"), "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output();
    output.is_ok_and(|out| {
        String::from_utf8_lossy(&out.stdout).to_ascii_lowercase().contains(&format!("\"{}\"", exe.to_ascii_lowercase()))
    })
}

/// Lets `thejar list` print to the terminal it was started from. The release build is a
/// GUI program, so Windows gives it no console of its own.
pub fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// The Jar's own install folder, from the entry its installer wrote.
pub fn self_install_location(app: &AppSpec) -> Option<PathBuf> {
    find(app).and_then(|entry| entry.location)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_display_names() {
        assert!(is_named("CrumbTree 0.2.0", "CrumbTree"));
        assert!(is_named("GameBridge", "GameBridge"));
        assert!(!is_named("GameBridge Pro", "GameBridge"));
        assert!(!is_named("CrumbTreeX 1.0", "CrumbTree"));
    }

    #[test]
    fn reads_icon_paths() {
        assert_eq!(icon_path(r#""C:\A\B.exe",0"#), PathBuf::from(r"C:\A\B.exe"));
        assert_eq!(icon_path(r"C:\A\B.exe"), PathBuf::from(r"C:\A\B.exe"));
    }
}
