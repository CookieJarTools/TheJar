//! The Jar updates itself from its own releases: on Linux by swapping its AppImage,
//! on Windows by running its new installer silently once it has closed.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

use crate::catalog::{AppSpec, LinuxSpec, WindowsSpec};
use crate::github::{self, Release};
use crate::http::Http;
use crate::platform;
use crate::version::Version;

pub const REPO: &str = "CookieJarTools/TheJar";
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
const UNMANAGED: &str = "This copy of The Jar didn't come from its installer or AppImage, so it can't update itself.";

pub fn spec() -> AppSpec {
    AppSpec {
        id: "thejar".to_owned(),
        name: "The Jar".to_owned(),
        tagline: "Installs and updates CookieJarTools apps.".to_owned(),
        repo: REPO.to_owned(),
        icon: None,
        windows: WindowsSpec {
            asset: Some("TheJar-Setup-*.exe".to_owned()),
            product_name: Some("The Jar".to_owned()),
            exe: Some("TheJar.exe".to_owned()),
        },
        linux: LinuxSpec {
            asset: Some("TheJar-*.AppImage".to_owned()),
            desktop_name: Some("thejar".to_owned()),
            categories: Some("Utility;".to_owned()),
            protocols: Vec::new(),
        },
    }
}

pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is valid")
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    #[cfg_attr(windows, allow(dead_code))]
    AppImage(PathBuf),
    #[cfg_attr(not(windows), allow(dead_code))]
    Installed,
    Unmanaged,
}

pub fn kind() -> Kind {
    #[cfg(not(windows))]
    {
        std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .map_or(Kind::Unmanaged, Kind::AppImage)
    }
    #[cfg(windows)]
    {
        let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from));
        let installed = platform::self_install_location(&spec());
        let same = |a: &std::path::Path, b: &std::path::Path| {
            let clean = |p: &std::path::Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
            clean(a) == clean(b)
        };
        match (exe_dir, installed) {
            (Some(exe_dir), Some(installed)) if same(&exe_dir, &installed) => Kind::Installed,
            _ => Kind::Unmanaged,
        }
    }
}

pub fn can_update_itself() -> bool {
    kind() != Kind::Unmanaged
}

/// A newer release of The Jar, if there is one.
pub fn newer(http: &Http, include_prereleases: bool) -> Result<Option<Release>, String> {
    let spec = spec();
    let release = github::latest(http, &spec.repo, spec.asset_pattern(), include_prereleases)?;
    Ok(release.filter(|release| release.version > current()))
}

#[derive(Clone, Debug)]
pub enum Ready {
    #[cfg_attr(windows, allow(dead_code))]
    Relaunch(PathBuf),
    RunInstaller(PathBuf),
}

pub fn download(
    http: &Http,
    release: &Release,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<Ready, String> {
    match kind() {
        Kind::AppImage(current) => replace_appimage(http, release, current, progress, cancel),
        Kind::Installed => {
            let file = platform::download_path(&spec(), &release.asset)?;
            http.download(&release.asset, &file, progress, cancel)?;
            Ok(Ready::RunInstaller(file))
        }
        Kind::Unmanaged => Err(UNMANAGED.to_owned()),
    }
}

#[cfg(not(windows))]
fn replace_appimage(
    http: &Http,
    release: &Release,
    current_file: PathBuf,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<Ready, String> {
    use std::os::unix::fs::PermissionsExt;

    let dir = current_file.parent().ok_or(UNMANAGED)?;
    let name = current_file.file_name().ok_or(UNMANAGED)?.to_string_lossy();
    // Keep the version in the file name truthful: TheJar-0.1.0.AppImage becomes TheJar-0.2.0.AppImage.
    let new_name = name.replace(&current().to_string(), &release.version.to_string());
    let dest = dir.join(&new_name);
    let temp = dir.join(format!(".{new_name}.download"));

    http.download(&release.asset, &temp, progress, cancel)?;
    std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Couldn't make the new version runnable: {e}"))?;
    std::fs::rename(&temp, &dest).map_err(|e| format!("Couldn't put the new version in place: {e}"))?;
    if dest != current_file {
        let _ = std::fs::remove_file(&current_file);
    }
    platform::integrate_self(&spec(), &dest, ICON_PNG);
    Ok(Ready::Relaunch(dest))
}

#[cfg(windows)]
fn replace_appimage(
    _: &Http,
    _: &Release,
    _: PathBuf,
    _: &mut dyn FnMut(u64, u64),
    _: &AtomicBool,
) -> Result<Ready, String> {
    Err(UNMANAGED.to_owned())
}

#[cfg(windows)]
fn installer_command(path: &std::path::Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(path);
    // --run is handled by The Jar's installer script: it opens The Jar again once it's done.
    command.raw_arg("/S").raw_arg("--run");
    command
}

#[cfg(not(windows))]
fn installer_command(path: &std::path::Path) -> Command {
    Command::new(path)
}

/// Starts the new version (or its installer). The caller should close The Jar straight after.
pub fn restart(ready: &Ready) -> Result<(), String> {
    let mut command = match ready {
        Ready::Relaunch(path) => {
            let mut command = Command::new(path);
            for var in ["APPIMAGE", "APPDIR", "OWD", "ARGV0"] {
                command.env_remove(var);
            }
            command
        }
        Ready::RunInstaller(path) => installer_command(path),
    };
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map(drop).map_err(|e| format!("Couldn't start the new version: {e}"))
}
