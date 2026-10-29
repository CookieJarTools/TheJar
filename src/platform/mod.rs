//! Installing, finding, opening and removing apps. Windows runs each app's own NSIS installer
//! silently; Linux keeps AppImages in The Jar's data folder and writes menu entries for them.

use std::path::PathBuf;

use crate::version::Version;

#[cfg(not(windows))]
mod linux;
#[cfg(not(windows))]
pub use linux::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub version: Option<Version>,
    pub version_text: String,
    /// The folder the app lives in.
    pub location: PathBuf,
    /// What to run to open it.
    pub program: PathBuf,
    /// Installed for everyone on a Windows PC, so its installer asks for admin rights.
    pub machine_wide: bool,
    pub origin: Origin,
}

/// How an app got onto this computer, which decides how The Jar updates and removes it.
#[derive(Clone, Debug, PartialEq)]
pub enum Origin {
    /// The Jar installed it, or on Windows the app's own installer did, which The Jar can rerun.
    Jar,
    /// Copied into the home folder some other way. The Jar updates and removes it where it is.
    Found,
    /// Installed by the system's package manager, or outside the home folder, so The Jar leaves
    /// it alone. Holds the package manager's name when one owns it.
    #[cfg_attr(windows, allow(dead_code))]
    System(Option<&'static str>),
}

impl Installed {
    /// Why The Jar won't update or remove it, when something else looks after it.
    pub fn kept_elsewhere(&self) -> Option<String> {
        match self.origin {
            Origin::System(Some(manager)) => {
                Some(format!("It was installed with {manager}, so update or remove it there."))
            }
            Origin::System(None) => Some(format!(
                "It's in {}, outside your home folder, so The Jar leaves it alone.",
                crate::paths::pretty(&self.location)
            )),
            Origin::Jar | Origin::Found => None,
        }
    }
}

/// Splits a Windows command line such as `"C:\App\Uninstall App.exe" /currentuser /S`
/// into the program and the rest, which is passed on untouched.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn split_command(command: &str) -> Option<(PathBuf, String)> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((PathBuf::from(&rest[..end]), rest[end + 1..].trim().to_owned()));
    }
    let lower = command.to_ascii_lowercase();
    let end = lower.find(".exe").map(|at| at + 4).unwrap_or(command.len());
    let program = command[..end].trim();
    (!program.is_empty()).then(|| (PathBuf::from(program), command[end..].trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::split_command;
    use std::path::PathBuf;

    #[test]
    fn splits_uninstall_commands() {
        let (program, args) =
            split_command(r#""C:\Users\Jo\AppData\Local\Programs\CrumbTree\Uninstall CrumbTree.exe" /currentuser /S"#)
                .unwrap();
        assert_eq!(program, PathBuf::from(r"C:\Users\Jo\AppData\Local\Programs\CrumbTree\Uninstall CrumbTree.exe"));
        assert_eq!(args, "/currentuser /S");

        let (program, args) = split_command(r"C:\Tools\uninst.exe /S").unwrap();
        assert_eq!(program, PathBuf::from(r"C:\Tools\uninst.exe"));
        assert_eq!(args, "/S");

        assert!(split_command(r#""unterminated"#).is_none());
        assert!(split_command("   ").is_none());
    }
}
