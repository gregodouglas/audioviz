//! Exercise the `platform` module without the rest of the app: prints the
//! power state, whether Spotify is installed, the diagnostics line and the
//! result of the manual update check. Never opens a browser or launches
//! Spotify; those two are only type-checked here.
//!
//! Run with `cargo run -p audioviz --example platform_probe`. Exits 0 even
//! when the update check fails (offline, rate limited) so it can be run
//! anywhere.

#[path = "../src/platform/mod.rs"]
mod platform;

fn main() {
    println!("on_battery        = {:?}", platform::on_battery());
    println!("spotify_installed = {}", platform::spotify_installed());
    println!("diagnostics       = {}", platform::diagnostics_header());

    match platform::latest_release("gregodouglas/audioviz") {
        Ok(release) => {
            let current = env!("CARGO_PKG_VERSION");
            println!("latest_release    = {} ({})", release.tag, release.url);
            println!("is_newer({:?}, {current:?}) = {}", release.tag, platform::is_newer(&release.tag, current));
        }
        Err(e) => println!("latest_release    = error: {e:#}"),
    }

    // Compile-check the two entry points that would pop windows on the
    // developer's machine; never call them from a probe.
    let _open_url: fn(&str) -> anyhow::Result<()> = platform::open_url;
    let _launch_spotify: fn() -> anyhow::Result<()> = platform::launch_spotify;
}
