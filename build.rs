fn main() {
    // Embed the Wisp icon + version info into Wisp.exe (Windows targets only).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Wisp");
        res.set("FileDescription", "Wisp");
        res.set("InternalName", "Wisp");
        res.set("OriginalFilename", "Wisp.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed icon: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
