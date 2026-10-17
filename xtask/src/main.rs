//! Repo chores. `cargo xtask icon` writes the app mark as SVG, PNG and ICO under
//! `assets/`. The mark is the phosphor scope's motif: a 3:2 Lissajous beam,
//! bright core with a green halo, on black.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context, bail};

mod mockup;

const ICO_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

fn main() -> anyhow::Result<()> {
    let task = std::env::args().nth(1).unwrap_or_default();
    match task.as_str() {
        "icon" => icon(),
        "mockup" => mockups(),
        _ => bail!("usage: cargo xtask <icon|mockup>"),
    }
}

/// Phase 6 UI direction as pictures (docs/phase6-ui.md).
fn mockups() -> anyhow::Result<()> {
    let img = repo_root().join("docs").join("img");
    std::fs::create_dir_all(&img)?;
    let (w, h) = (1280.0, 800.0);
    let frames: [(&str, String, f32); 7] = [
        ("ui-mockup", mockup::frame_panel(w, h), 1.0),
        ("ui-hover", mockup::frame_hover(w, h), 1.0),
        ("ui-ambient", mockup::frame_ambient(w, h), 1.0),
        ("ui-empty", mockup::frame_empty(w, h), 1.0),
        ("ui-calibrate", mockup::frame_calibrate(w, h), 1.0),
        ("installer-mockup", mockup::installer_install_svg(), 1.5),
        ("installer-finish", mockup::installer_finish_svg(), 1.5),
    ];
    for (name, svg, scale) in &frames {
        std::fs::write(img.join(format!("{name}.svg")), svg)?;
        mockup::render_png(svg, *scale, &img.join(format!("{name}.png")))?;
    }
    println!("wrote {}", img.display());
    Ok(())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// Lissajous x = sin(3t + pi/2), y = sin(2t), centered in a 512 box.
fn lissajous_path() -> String {
    let n = 720;
    let (cx, cy, r) = (256.0, 256.0, 150.0);
    let mut d = String::new();
    for i in 0..=n {
        let t = i as f64 / n as f64 * std::f64::consts::TAU;
        let x = cx + r * (3.0 * t + std::f64::consts::FRAC_PI_2).sin() * 1.05;
        let y = cy + r * (2.0 * t).sin();
        let _ = write!(d, "{}{:.1} {:.1} ", if i == 0 { "M" } else { "L" }, x, y);
    }
    d.push('Z');
    d
}

fn svg() -> String {
    let path = lissajous_path();
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <!-- audioviz mark: phosphor-scope Lissajous on black. Regenerate with `cargo xtask icon`. -->
  <rect width="512" height="512" rx="104" fill="#06090b"/>
  <g fill="none" stroke-linecap="round" stroke-linejoin="round">
    <path d="{path}" stroke="#2fd66a" stroke-opacity="0.16" stroke-width="56"/>
    <path d="{path}" stroke="#4dff87" stroke-opacity="0.38" stroke-width="24"/>
    <path d="{path}" stroke="#66ff8c" stroke-width="10"/>
    <path d="{path}" stroke="#e6fff0" stroke-opacity="0.9" stroke-width="3.5"/>
  </g>
</svg>
"##
    )
}

fn render(tree: &resvg::usvg::Tree, size: u32) -> anyhow::Result<Vec<u8>> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).context("pixmap")?;
    let scale = size as f32 / tree.size().width();
    resvg::render(tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect())
}

fn icon() -> anyhow::Result<()> {
    let assets = repo_root().join("assets");
    std::fs::create_dir_all(&assets)?;
    let svg_text = svg();
    std::fs::write(assets.join("icon.svg"), &svg_text)?;

    let tree = resvg::usvg::Tree::from_str(&svg_text, &resvg::usvg::Options::default()).context("parse svg")?;

    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in &ICO_SIZES {
        let rgba = render(&tree, size)?;
        let image = ico::IconImage::from_rgba_data(size, size, rgba);
        let entry = if size >= 256 { ico::IconDirEntry::encode_as_png(&image)? } else { ico::IconDirEntry::encode(&image)? };
        dir.add_entry(entry);
    }
    let ico_path = assets.join("icon.ico");
    dir.write(std::fs::File::create(&ico_path)?)?;

    // 256 px PNG for the winit window icon and docs.
    let rgba = render(&tree, 256)?;
    let image = ico::IconImage::from_rgba_data(256, 256, rgba);
    image.write_png(std::fs::File::create(assets.join("icon-256.png"))?)?;

    println!("wrote {}", assets.display());
    Ok(())
}
