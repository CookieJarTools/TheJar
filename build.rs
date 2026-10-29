fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Gives TheJar.exe its icon in Explorer and the taskbar. This needs the Windows SDK's rc.exe,
    // so it only runs on a Windows machine, not when checking the Windows code from Linux.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.set("ProductName", "The Jar");
        resource.set("FileDescription", "The Jar");
        resource.set("LegalCopyright", "Copyright 2026 Joseph Cook");
        resource.compile().expect("couldn't embed the icon in TheJar.exe");
    }
}
