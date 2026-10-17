// Windows: embed the app icon and version resource into audioviz.exe.
// The icon comes from `cargo xtask icon` (assets/icon.ico). On other targets
// this is a no-op.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.set("ProductName", "audioviz");
        res.set("FileDescription", "audioviz music visualizer");
        res.set("LegalCopyright", "Copyright (c) 2026 Gregory Douglas. MIT License.");
        res.set("OriginalFilename", "audioviz.exe");
        res.set("InternalName", "audioviz");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources (icon/version): {e}");
        }
    }
}
