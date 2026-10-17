// No console window in release builds; debug keeps it for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bandfilter;
mod chrome;
mod config;
mod director;
mod engine;
mod gpu;
mod hittest;
mod overlays;
mod panel;
mod platform;
mod post;
mod scene;
mod scene_store;
mod scenes;
mod settings;
mod shaders;
mod strip;
mod theme;
mod tray;
mod widgets;

use tracing_subscriber::EnvFilter;
use winit::event_loop::{ControlFlow, EventLoop};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
audioviz {version} - a spacey, line-based music visualizer

usage: audioviz [--version] [--help] [--tray] [--set key=value]...

  --tray            start minimised (for 'start when I sign in')
  --set key=value   change a setting in settings.json before starting,
                    e.g. --set start_fullscreen=true --set monitor=1

keys: Tab panel · H hide everything · 1-3 layer to edit · Space next scene · A auto scenes · N now playing
      L next look · C calibrate audio delay · ? shortcuts · F11 fullscreen · F5 reload shaders
      Esc closes sheet / panel / fullscreen, then quits
settings: %APPDATA%\\audioviz\\settings.json (Windows)
env: RUST_LOG=audioviz=debug for a per-second stats line
     AUDIOVIZ_SCENE=n AUDIOVIZ_SIZE=WxH AUDIOVIZ_DEFAULTS=1 (dev overrides)
";

struct Cli {
    tray: bool,
    sets: Vec<(String, String)>,
}

#[cfg(windows)]
fn attach_console() {
    unsafe {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
        let inherited = GetStdHandle(STD_OUTPUT_HANDLE).map(|h| !h.is_invalid()).unwrap_or(false);
        if !inherited {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
#[cfg(not(windows))]
fn attach_console() {}

/// Parse the command line. Prints and returns None for --version / --help / errors.
fn parse_cli() -> Option<Cli> {
    let mut cli = Cli { tray: false, sets: Vec::new() };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--version" | "-V" => {
                attach_console();
                print!("audioviz {VERSION}\n");
                return None;
            }
            "--help" | "-h" => {
                attach_console();
                print!("{}", HELP.replace("{version}", VERSION));
                return None;
            }
            "--tray" => cli.tray = true,
            "--set" => match args.next().and_then(|kv| kv.split_once('=').map(|(k, v)| (k.to_string(), v.to_string()))) {
                Some(kv) => cli.sets.push(kv),
                None => {
                    attach_console();
                    print!("audioviz: --set needs key=value\n");
                    return None;
                }
            },
            other => {
                attach_console();
                print!("audioviz: unknown option {other}\n{}", HELP.replace("{version}", VERSION));
                return None;
            }
        }
    }
    Some(cli)
}

fn main() -> anyhow::Result<()> {
    let Some(cli) = parse_cli() else { return Ok(()) };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,naga=warn")))
        .init();
    tracing::info!("audioviz {VERSION}");
    if !cli.sets.is_empty() {
        config::apply_overrides(&cli.sets);
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = app::App::new(cli.tray);
    event_loop.run_app(&mut app)?;
    Ok(())
}
