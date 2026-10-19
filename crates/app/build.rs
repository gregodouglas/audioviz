// Build script.
//
// 1. Factory presets, scenes and loadable visuals: every JSON and WGSL file
//    under `assets/presets/`, `assets/scenes/` and `assets/visuals/` is
//    embedded, so the binary can write them into the user's folder on first
//    run and on "Restore factory". The list is
//    generated here so adding a file is all it takes.
// 2. Windows: embed the app icon and version resource into audioviz.exe.
//    The icon comes from `cargo xtask icon` (assets/icon.ico).

use std::path::{Path, PathBuf};

fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if p.is_dir() {
            walk(&p, &r, out);
        } else if matches!(p.extension().and_then(|x| x.to_str()), Some("json" | "wgsl")) {
            out.push((r, p));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    println!("cargo:rerun-if-changed=../../assets/presets");
    println!("cargo:rerun-if-changed=../../assets/scenes");
    println!("cargo:rerun-if-changed=../../assets/visuals");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest.join("../../assets");
    let mut files = Vec::new();
    walk(&assets.join("presets"), "presets", &mut files);
    walk(&assets.join("scenes"), "scenes", &mut files);
    walk(&assets.join("visuals"), "visuals", &mut files);
    for (_, p) in &files {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    let mut src = String::from("/// (relative path under the audioviz config folder, contents)\npub const FACTORY: &[(&str, &str)] = &[\n");
    for (rel, p) in &files {
        let abs = p.canonicalize().unwrap_or_else(|_| p.clone());
        let abs = abs.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/");
        src.push_str(&format!("    ({rel:?}, include_str!({abs:?})),\n"));
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("factory_list.rs");
    std::fs::write(out, src).unwrap();

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
