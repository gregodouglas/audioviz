//! Read-only check of the settings migration against a real file:
//!   cargo run -p audioviz --example settings_probe -- path\to\settings.json
//! Prints the layers, post and a few fields after migration. Never writes.

#[path = "../src/color.rs"]
#[allow(dead_code)]
mod color;
#[path = "../src/settings.rs"]
#[allow(dead_code)]
mod settings;
#[path = "../src/config.rs"]
#[allow(dead_code)]
mod config;
#[path = "../src/links.rs"]
#[allow(dead_code)]
mod links;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage: settings_probe <settings.json>"))?;
    let text = std::fs::read_to_string(&path)?;
    let s = config::parse(&text)?;
    println!("scene_name = {:?}  editing = {}  section = {:?}", s.scene_name, s.editing, s.section);
    for (i, l) in s.layers.iter().enumerate() {
        println!("layer {}: {:?} enabled={} opacity={:.2} band={}", i + 1, l.visual, l.enabled, l.opacity, l.band.label());
        for (id, params) in &l.params {
            println!("   {id} {params}");
        }
    }
    println!("post = {:?}", s.post);
    println!("tint = {:?}  delay = {} ms  monitor = {}  fps_cap = {}  show_ui = {}", s.tint, s.audio_delay_ms, s.monitor, s.fps_cap, s.show_ui);
    Ok(())
}
