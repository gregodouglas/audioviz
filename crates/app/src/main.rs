// No console window in release builds; debug keeps it for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod director;
mod engine;
mod gpu;
mod overlay;
mod post;
mod scene;
mod scenes;
mod shaders;
mod ui;

use tracing_subscriber::EnvFilter;
use winit::event_loop::{ControlFlow, EventLoop};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
audioviz {version} - a spacey, line-based music visualizer

usage: audioviz [--version] [--help]

keys: Tab panel · 1-3/Space scenes · A auto scenes · T title overlay
      C calibrate audio delay · F11 fullscreen · F5 reload shaders · Esc quit
settings: %APPDATA%\\audioviz\\settings.json (Windows)
env: RUST_LOG=audioviz=debug for a per-second stats line
     AUDIOVIZ_SCENE=n AUDIOVIZ_SIZE=WxH AUDIOVIZ_DEFAULTS=1 (dev overrides)
";

/// Handle `--version` / `--help` before touching the GPU. A windows-subsystem
/// binary has no console, so attach to the parent's (cmd/PowerShell) first.
fn handle_cli() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text = match args.first().map(String::as_str) {
        Some("--version" | "-V") => format!("audioviz {VERSION}\n"),
        Some("--help" | "-h") => HELP.replace("{version}", VERSION),
        Some(other) => format!("audioviz: unknown option {other}\n{}", HELP.replace("{version}", VERSION)),
        None => return false,
    };
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
        // If stdout was redirected (pipe/file) the handle is already inherited;
        // only a bare console launch needs to attach to the parent's console.
        let inherited = GetStdHandle(STD_OUTPUT_HANDLE).map(|h| !h.is_invalid()).unwrap_or(false);
        if !inherited {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
    print!("{text}");
    true
}

fn main() -> anyhow::Result<()> {
    if handle_cli() {
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,naga=warn")))
        .init();
    tracing::info!("audioviz {VERSION}");

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = app::App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
