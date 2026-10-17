//! `cargo xtask installer-images`: renders the Inno Setup wizard images into
//! `assets/installer/` from the design tokens in `mockup.rs`. The large image
//! is the dark strip on the Welcome/Finished pages (164x314 nominal), the small
//! one sits in the header of every other page (55x58). Inno picks the closest
//! size to what the DPI + WizardSizePercent ask for and scales it, so each image
//! is written at 100 %, 150 % and 200 %. Rendered at 2x and downsampled with
//! Lanczos so the strokes stay crisp. 24-bit BMP, which is what
//! WizardImageFile wants on every Inno 6 release.

use anyhow::Context;

use crate::mockup::{ACCENT, BG, MONO, SANS, TEXT, TEXT_3, lissajous};

const LARGE: (u32, u32) = (164, 314);
const SMALL: (u32, u32) = (55, 58);
const SCALES: [(&str, f32); 3] = [("", 1.0), ("-150", 1.5), ("-200", 2.0)];

pub fn run() -> anyhow::Result<()> {
    let out = crate::repo_root().join("assets").join("installer");
    std::fs::create_dir_all(&out)?;
    let large = large_svg();
    let small = small_svg();
    std::fs::write(out.join("wizard-large.svg"), &large)?;
    std::fs::write(out.join("wizard-small.svg"), &small)?;
    for (suffix, scale) in SCALES {
        render_bmp(
            &large,
            LARGE,
            scale,
            &out.join(format!("wizard-large{suffix}.bmp")),
        )?;
        render_bmp(
            &small,
            SMALL,
            scale,
            &out.join(format!("wizard-small{suffix}.bmp")),
        )?;
    }
    println!("wrote {}", out.display());
    Ok(())
}

/// The mark with a soft glow: a blurred wide stroke under the crisp core.
fn mark(cx: f32, cy: f32, r: f32) -> String {
    let p = lissajous(cx, cy, r);
    let w = r / 9.0;
    format!(
        concat!(
            r##"<path d="{p}" fill="none" stroke="{a}" stroke-opacity="0.55" stroke-width="{glow:.2}" stroke-linejoin="round" filter="url(#glow)"/>"##,
            r##"<path d="{p}" fill="none" stroke="{a}" stroke-opacity="0.35" stroke-width="{halo:.2}" stroke-linejoin="round"/>"##,
            r##"<path d="{p}" fill="none" stroke="{a}" stroke-width="{w:.2}" stroke-linejoin="round"/>"##,
        ),
        p = p,
        a = ACCENT,
        glow = w * 4.0,
        halo = w * 2.2,
        w = w,
    )
}

fn glow_filter(std_dev: f32) -> String {
    format!(
        r##"<defs><filter id="glow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="{std_dev:.2}"/></filter></defs>"##
    )
}

/// Welcome/Finished strip: mark, "audioviz", licence line at the foot.
fn large_svg() -> String {
    let (w, h) = (LARGE.0 as f32, LARGE.1 as f32);
    let cx = w * 0.5;
    format!(
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"##,
            "{filter}",
            r##"<rect width="{w}" height="{h}" fill="{bg}"/>"##,
            "{mark}",
            r##"<text x="{cx}" y="{ty:.1}" font-family="{sans}" font-size="15" font-weight="600" fill="{text}" text-anchor="middle">audioviz</text>"##,
            r##"<text x="{cx}" y="{fy:.1}" font-family="{mono}" font-size="8.5" fill="{text3}" text-anchor="middle">MIT · no telemetry</text>"##,
            "</svg>",
        ),
        w = w,
        h = h,
        filter = glow_filter(3.0),
        bg = BG,
        mark = mark(cx, 118.0, 40.0),
        cx = cx,
        ty = 198.0,
        sans = SANS,
        text = TEXT,
        fy = h - 16.0,
        mono = MONO,
        text3 = TEXT_3,
    )
}

/// Header image: just the mark on the dark ground.
fn small_svg() -> String {
    let (w, h) = (SMALL.0 as f32, SMALL.1 as f32);
    format!(
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"##,
            "{filter}",
            r##"<rect width="{w}" height="{h}" fill="{bg}"/>"##,
            "{mark}",
            "</svg>",
        ),
        w = w,
        h = h,
        filter = glow_filter(1.2),
        bg = BG,
        mark = mark(w * 0.5, h * 0.5, 16.0),
    )
}

/// Render `svg` so the output is `base * scale` pixels, going through a 2x
/// supersample, and save as 24-bit BMP.
fn render_bmp(
    svg: &str,
    base: (u32, u32),
    scale: f32,
    out: &std::path::Path,
) -> anyhow::Result<()> {
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    opt.font_family = "Segoe UI".into();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).context("parse svg")?;
    let (tw, th) = (
        (base.0 as f32 * scale).round() as u32,
        (base.1 as f32 * scale).round() as u32,
    );
    let (rw, rh) = (tw * 2, th * 2);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(rw, rh).context("pixmap")?;
    let s = rw as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(s, s),
        &mut pixmap.as_mut(),
    );
    // Flatten onto BG (the SVG already fills it, but premultiplied edges need an opaque ground).
    let bg = {
        let v = u32::from_str_radix(BG.trim_start_matches('#'), 16)?;
        [(v >> 16) as u8, (v >> 8) as u8, v as u8]
    };
    let mut rgb = image::RgbImage::new(rw, rh);
    for (i, p) in pixmap.pixels().iter().enumerate() {
        let c = p.demultiply();
        let a = c.alpha() as u32;
        let blend = |ch: u8, ground: u8| ((ch as u32 * a + ground as u32 * (255 - a)) / 255) as u8;
        let px = image::Rgb([
            blend(c.red(), bg[0]),
            blend(c.green(), bg[1]),
            blend(c.blue(), bg[2]),
        ]);
        rgb.put_pixel(i as u32 % rw, i as u32 / rw, px);
    }
    let small = image::imageops::resize(&rgb, tw, th, image::imageops::FilterType::Lanczos3);
    small
        .save_with_format(out, image::ImageFormat::Bmp)
        .with_context(|| format!("write {}", out.display()))?;
    Ok(())
}
