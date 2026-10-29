// The release build on Windows is a GUI program, so double-clicking it doesn't open a console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod catalog;
mod cli;
mod github;
mod gui;
mod http;
mod icons;
mod ops;
mod paths;
mod platform;
mod selfupdate;
mod settings;
mod version;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        #[cfg(windows)]
        platform::attach_console();
        std::process::exit(cli::run(&args));
    }
    if let Err(err) = gui::run() {
        eprintln!("The Jar couldn't open its window: {err}");
        std::process::exit(1);
    }
}
