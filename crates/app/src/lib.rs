//! The engine and the app: `audioviz` (src/main.rs) runs the app, and
//! `audioviz-lab` (src/bin/lab.rs, ADR-0003) renders one visual on its own
//! for design work.

pub mod app;
pub mod bandfilter;
pub mod chrome;
pub mod color;
pub mod config;
pub mod director;
pub mod engine;
pub mod factory;
pub mod gpu;
pub mod gpu_timer;
pub mod hittest;
pub mod overlays;
pub mod panel;
pub mod pictures;
pub mod platform;
pub mod post;
pub mod preset_store;
pub mod rng;
pub mod scene;
pub mod scene_store;
pub mod settings;
pub mod shader_visual;
pub mod shaders;
pub mod strip;
pub mod theme;
pub mod tray;
pub mod visuals;
pub mod widgets;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
