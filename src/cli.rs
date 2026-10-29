//! `thejar list`, `thejar update` and friends, for scripts and for anyone who'd rather type.

use std::io::Write;
use std::sync::atomic::AtomicBool;

use crate::catalog::{self, AppSpec, Source};
use crate::github::{self, Target};
use crate::http::{Http, size_text};
use crate::ops::{self, Phase};
use crate::platform::{self, Installed, Origin};
use crate::selfupdate::{self, Ready};
use crate::settings::Settings;

const HELP: &str = "The Jar installs and updates CookieJarTools apps.

Usage: thejar [command]

With no command, The Jar opens its window.

Commands:
  list                 Show each app, what's installed and what's available
  install <app>...     Install apps, or update them if they're already installed
  update [<app>...]    Update the named apps, or every installed app
  uninstall <app>...   Uninstall apps (their settings and data are kept)
  open <app>           Open an installed app
  self-update          Update The Jar itself
  help                 Show this
  version              Show The Jar's version";

struct Session {
    http: Http,
    settings: Settings,
    apps: Vec<AppSpec>,
}

pub fn run(args: &[String]) -> i32 {
    let (command, rest) = (args[0].as_str(), &args[1..]);
    let result = match command {
        "list" | "ls" => list(),
        "install" => install(rest, false),
        "update" | "upgrade" => install(rest, true),
        "uninstall" | "remove" => uninstall(rest),
        "open" | "launch" => open(rest),
        "self-update" => self_update(),
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("The Jar {}", selfupdate::current());
            Ok(())
        }
        _ => Err(format!("There's no \"{command}\" command. Run `thejar help` to see what The Jar can do.")),
    };
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn session() -> Session {
    let http = Http::new();
    let loaded = catalog::load(&http);
    match &loaded.source {
        Source::Saved(reason) => eprintln!("Using the app list saved last time. {reason}"),
        Source::Bundled(reason) => eprintln!("Using the app list built into The Jar. {reason}"),
        Source::Remote => {}
    }
    Session { http, settings: Settings::load(), apps: loaded.apps }
}

fn find<'a>(apps: &'a [AppSpec], name: &str) -> Result<&'a AppSpec, String> {
    apps.iter()
        .find(|app| app.id.eq_ignore_ascii_case(name) || app.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("There's no app called \"{name}\". Run `thejar list` to see them."))
}

fn list() -> Result<(), String> {
    let session = session();
    let mut rows = vec![["APP".to_owned(), "ID".to_owned(), "INSTALLED".to_owned(), "LATEST".to_owned()]];
    for app in &session.apps {
        let installed = platform::detect(app);
        let latest =
            github::latest(&session.http, &app.repo, app.asset_pattern(), session.settings.include_prereleases);
        let installed_text = installed.as_ref().map_or("-".to_owned(), installed_text);
        let latest_text = match latest {
            Ok(Some(release)) => {
                let newer = installed.as_ref().is_some_and(|i| i.version.as_ref().is_none_or(|v| release.version > *v));
                if newer { format!("{} (update available)", release.version) } else { release.version.to_string() }
            }
            Ok(None) => format!("no {} build yet", Target::current().os_name()),
            Err(err) => format!("couldn't check: {err}"),
        };
        rows.push([app.name.clone(), app.id.clone(), installed_text, latest_text]);
    }

    let widths: Vec<usize> = (0..3).map(|col| rows.iter().map(|r| r[col].chars().count()).max().unwrap_or(0)).collect();
    for row in rows {
        println!(
            "{:w0$}   {:w1$}   {:w2$}   {}",
            row[0],
            row[1],
            row[2],
            row[3],
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2]
        );
    }
    Ok(())
}

/// The version, and who looks after it when that isn't The Jar.
fn installed_text(installed: &Installed) -> String {
    let version = if installed.version_text.is_empty() { "unknown" } else { &installed.version_text };
    match installed.origin {
        Origin::Jar => version.to_owned(),
        Origin::Found => format!("{version} (not from The Jar)"),
        Origin::System(Some(manager)) => format!("{version} ({manager})"),
        Origin::System(None) => format!("{version} (system)"),
    }
}

fn install(names: &[String], update_only: bool) -> Result<(), String> {
    let session = session();
    let targets: Vec<&AppSpec> = if names.is_empty() || names == ["--all"] {
        if !update_only {
            return Err("Name an app to install, such as `thejar install crumbtree`.".to_owned());
        }
        session.apps.iter().filter(|app| updates_with_the_rest(app)).collect()
    } else {
        names.iter().map(|name| find(&session.apps, name)).collect::<Result<_, _>>()?
    };
    if targets.is_empty() {
        println!("Nothing is installed yet.");
        return Ok(());
    }

    let mut failed = 0;
    for app in targets {
        if let Err(err) = install_one(&session, app, update_only) {
            eprintln!("{}: {err}", app.name);
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(format!("{failed} of them didn't finish."));
    }
    Ok(())
}

/// Whether `thejar update` with no names should update it. A copy The Jar didn't install and
/// can't tell the version of is only replaced when it's named.
fn updates_with_the_rest(app: &AppSpec) -> bool {
    match platform::detect(app) {
        Some(installed) if installed.origin == Origin::Found && installed.version.is_none() => {
            println!(
                "{}: The Jar didn't install the copy in {} and can't tell its version, so it's left alone. \
                 `thejar install {}` replaces it.",
                app.name,
                crate::paths::pretty(&installed.location),
                app.id
            );
            false
        }
        installed => installed.is_some(),
    }
}

fn install_one(session: &Session, app: &AppSpec, update_only: bool) -> Result<(), String> {
    let installed = platform::detect(app);
    if installed.is_none() && update_only {
        println!("{} isn't installed. Use `thejar install {}`.", app.name, app.id);
        return Ok(());
    }
    if let Some(reason) = installed.as_ref().and_then(Installed::kept_elsewhere) {
        if update_only {
            println!("{}: {reason}", app.name);
            return Ok(());
        }
        return Err(reason);
    }
    let Some(release) =
        github::latest(&session.http, &app.repo, app.asset_pattern(), session.settings.include_prereleases)?
    else {
        println!("{} has no {} build yet.", app.name, Target::current().os_name());
        return Ok(());
    };
    if let Some(version) = installed.as_ref().and_then(|i| i.version.as_ref())
        && *version >= release.version
    {
        println!("{} {} is up to date.", app.name, version);
        return Ok(());
    }
    if let Some(current) = &installed
        && platform::CLOSES_APP_TO_UPDATE
        && platform::is_running(app, current)
    {
        return Err("it's open. Close it, then run this again.".to_owned());
    }

    let verb = if installed.is_some() { "Updating" } else { "Installing" };
    eprintln!("{verb} {} {} ({})", app.name, release.version, size_text(release.asset.size));
    let mut last_percent = None;
    let mut report = |phase: Phase, done: u64, total: u64| {
        let mut err = std::io::stderr();
        match phase {
            Phase::Downloading if total > 0 => {
                let percent = done * 100 / total;
                if last_percent != Some(percent) {
                    last_percent = Some(percent);
                    let _ = write!(err, "\r  Downloading {percent:>3}%  {} of {}  ", size_text(done), size_text(total));
                }
            }
            Phase::Installing => {
                let _ = writeln!(err, "\n  Installing…");
            }
            _ => {}
        }
        let _ = err.flush();
    };
    let cancel = AtomicBool::new(false);
    let result = ops::install(&session.http, app, &release, installed.as_ref(), &mut report, &cancel);
    if result.is_err() {
        eprintln!();
    }
    let now = result?;
    let where_to = now.map(|i| crate::paths::pretty(&i.location)).unwrap_or_default();
    println!("{} {} is installed in {where_to}.", app.name, release.version);
    Ok(())
}

fn uninstall(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("Name an app to uninstall, such as `thejar uninstall crumbtree`.".to_owned());
    }
    let session = session();
    for name in names {
        let app = find(&session.apps, name)?;
        let Some(installed) = platform::detect(app) else {
            println!("{} isn't installed.", app.name);
            continue;
        };
        ops::uninstall(app, &installed, &mut |_, _, _| {}).map_err(|err| format!("{}: {err}", app.name))?;
        println!("Uninstalled {}. Its settings and data are still there.", app.name);
    }
    Ok(())
}

fn open(names: &[String]) -> Result<(), String> {
    let [name] = names else {
        return Err("Name one app to open, such as `thejar open crumbtree`.".to_owned());
    };
    let apps = catalog::load_saved().apps;
    let app = find(&apps, name)?;
    let installed = platform::detect(app).ok_or_else(|| format!("{} isn't installed.", app.name))?;
    platform::launch(&installed)
}

fn self_update() -> Result<(), String> {
    let http = Http::new();
    let settings = Settings::load();
    let Some(release) = selfupdate::newer(&http, settings.include_prereleases)? else {
        println!("The Jar {} is up to date.", selfupdate::current());
        return Ok(());
    };
    if !selfupdate::can_update_itself() {
        return Err(format!(
            "The Jar {} is out, but this copy can't replace itself. Download it from {}",
            release.version, release.page_url
        ));
    }
    eprintln!("Downloading The Jar {} ({})", release.version, size_text(release.asset.size));
    let ready = selfupdate::download(&http, &release, &mut |_, _| {}, &AtomicBool::new(false))?;
    match &ready {
        Ready::Relaunch(path) => {
            println!("The Jar {} is now at {}.", release.version, crate::paths::pretty(path));
        }
        Ready::RunInstaller(_) => {
            selfupdate::restart(&ready)?;
            println!("Installing The Jar {}. It opens again when it's done.", release.version);
        }
    }
    Ok(())
}
