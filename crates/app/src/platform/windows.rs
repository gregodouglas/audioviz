//! Windows implementation of the platform layer. Everything goes through the
//! `windows` crate's bindings to OS DLLs that are already loaded in any
//! process (shell32, kernel32, advapi32) plus winhttp.dll for the one manual
//! update check. No Rust networking crate is linked; `cargo deny check bans`
//! enforces that.

use std::ffi::c_void;
use std::path::PathBuf;

use anyhow::{Context, anyhow, bail};
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CLASSES_ROOT, HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegOpenKeyExW,
    RegQueryValueExW,
};
use windows::Win32::System::SystemInformation::{
    GetNativeSystemInfo, PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM64, PROCESSOR_ARCHITECTURE_INTEL, SYSTEM_INFO,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use super::{ReleaseInfo, parse_release_json};

/// Null-terminated UTF-16 for the `W` APIs.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `ShellExecuteW(NULL, "open", target, ...)`. The return value is an
/// `HINSTANCE` by name only: anything above 32 is success, lower values are
/// error codes (`SE_ERR_*`).
fn shell_open(target: &str) -> anyhow::Result<()> {
    let verb = wide("open");
    let target_w = wide(target);
    // SAFETY: both strings are null-terminated and outlive the call; the
    // optional parameters are null, which ShellExecuteW documents as allowed.
    let rc = unsafe {
        ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(target_w.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
    };
    let code = rc.0 as usize;
    if code > 32 {
        Ok(())
    } else {
        let why = match code {
            2 => "file not found",
            3 => "path not found",
            5 => "access denied",
            8 => "out of memory",
            31 => "no application is associated with this type",
            32 => "DLL not found",
            _ => "ShellExecute failed",
        };
        bail!("could not open {target}: {why} (code {code})")
    }
}

/// Open `url` in the default browser (used for the releases page, PRIVACY.md,
/// the issue tracker). Only `http://` and `https://` are accepted so a
/// mistyped constant can never ShellExecute an arbitrary file or protocol.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        bail!("refusing to open non-web URL {url:?}");
    }
    shell_open(url)
}

/// Open the Windows "Cast" panel (the same one Win+K shows) so the user can
/// pick a Miracast receiver such as a Roku TV. The TV then appears as a
/// wireless display; Windows carries the audio with it.
pub fn open_cast_picker() -> anyhow::Result<()> {
    shell_open("ms-settings-connectabledevices:devicediscovery")
}

/// Candidate Spotify executables, in the order we try them: the desktop
/// installer's `%APPDATA%\Spotify\Spotify.exe`, then the Microsoft Store
/// app-execution alias under `%LOCALAPPDATA%\Microsoft\WindowsApps`.
fn spotify_exe_candidates() -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(2);
    if let Some(appdata) = std::env::var_os("APPDATA") {
        out.push(PathBuf::from(appdata).join("Spotify").join("Spotify.exe"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(PathBuf::from(local).join("Microsoft").join("WindowsApps").join("Spotify.exe"));
    }
    out
}

/// Whether the `spotify:` URI scheme has a registered handler (`HKCR\spotify`).
fn spotify_uri_registered() -> bool {
    reg_open(HKEY_CLASSES_ROOT, "spotify").is_some()
}

/// Start Spotify. Tries, in order: the `spotify:` URI (whatever handler is
/// registered, Store or desktop), then `%APPDATA%\Spotify\Spotify.exe`, then
/// the Store alias `%LOCALAPPDATA%\Microsoft\WindowsApps\Spotify.exe`. Returns
/// the error from the last attempt if none of them worked.
pub fn launch_spotify() -> anyhow::Result<()> {
    let mut last_err = None;
    if spotify_uri_registered() {
        match shell_open("spotify:") {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    for exe in spotify_exe_candidates() {
        if !exe.is_file() {
            continue;
        }
        match shell_open(&exe.to_string_lossy()) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("Spotify does not appear to be installed (no spotify: handler and no Spotify.exe)")))
}

/// Whether Spotify looks installed: a `spotify:` URI handler in the registry
/// or one of the known `Spotify.exe` locations exists. Never launches anything.
pub fn spotify_installed() -> bool {
    spotify_uri_registered() || spotify_exe_candidates().iter().any(|p| p.is_file())
}

/// `Some(true)` when running on battery, `Some(false)` on mains power, `None`
/// when the machine has no battery (desktop) or the state is unknown.
pub fn on_battery() -> Option<bool> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` is a valid, writable struct of the documented size.
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
    // BatteryFlag 128 = no system battery, 255 = unknown.
    if status.BatteryFlag & 128 != 0 || status.BatteryFlag == 255 {
        return None;
    }
    match status.ACLineStatus {
        0 => Some(true),
        1 => Some(false),
        _ => None,
    }
}

// ---- update check (WinHTTP) -------------------------------------------------

/// Owned WinHTTP handle, closed on drop so every early return cleans up.
struct WinHttpHandle(*mut c_void);

impl WinHttpHandle {
    fn new(raw: *mut c_void, what: &str) -> anyhow::Result<Self> {
        if raw.is_null() {
            let err = windows::core::Error::from_thread();
            bail!("{what} failed: {}", describe_winhttp_error(&err));
        }
        Ok(Self(raw))
    }
}

impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a WinHTTP open/connect call and is
        // closed exactly once, here.
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// Turn the common WinHTTP failure codes into something a user can act on.
fn describe_winhttp_error(err: &windows::core::Error) -> String {
    // HRESULT_FROM_WIN32 keeps the Win32 code in the low 16 bits.
    let code = (err.code().0 as u32) & 0xFFFF;
    match code {
        12002 => "timed out after 5 s".to_owned(),
        12007 => "could not resolve api.github.com (offline?)".to_owned(),
        12029 | 12030 => "could not connect to api.github.com (offline or blocked?)".to_owned(),
        12175 | 12188 => "TLS handshake failed (proxy or clock problem?)".to_owned(),
        _ => {
            let msg = err.message();
            let msg = msg.trim();
            if msg.is_empty() { format!("{err}") } else { msg.to_owned() }
        }
    }
}

const TIMEOUT_MS: i32 = 5_000;
/// A release JSON is a few KiB; refuse anything absurd rather than buffer it.
const MAX_BODY: usize = 1 << 20;

/// Fetch the newest published release of `owner_repo` (e.g.
/// `"gregodouglas/audioviz"`) from the GitHub REST API over HTTPS with WinHTTP.
///
/// This is the only network call in the app and it runs **only** when the
/// user presses "Check for updates" (see PRIVACY.md). Synchronous with 5 s
/// resolve / connect / send / receive timeouts, so call it from a worker
/// thread. Never panics on network trouble: offline, DNS, TLS, rate limiting
/// (HTTP 403/429), missing release (404) and bad JSON all come back as `Err`
/// with a readable message.
pub fn latest_release(owner_repo: &str) -> anyhow::Result<ReleaseInfo> {
    // Keep the path we splice into the URL to the characters GitHub names use.
    let valid = owner_repo.split('/').count() == 2
        && owner_repo.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if !valid {
        bail!("invalid repository name {owner_repo:?}");
    }

    let body = winhttp_get("api.github.com", &format!("/repos/{owner_repo}/releases/latest"))?;
    match body.status {
        200 => parse_release_json(&body.bytes).context("could not read the release information GitHub sent"),
        403 | 429 => bail!("GitHub rate limit reached (HTTP {}); try again in a few minutes", body.status),
        404 => bail!("no published release found for {owner_repo}"),
        other => bail!("GitHub returned HTTP {other}"),
    }
}

struct Response {
    status: u32,
    bytes: Vec<u8>,
}

/// One HTTPS GET of `path` on `host`, with the headers GitHub wants. Every
/// handle is owned by a [`WinHttpHandle`] and all buffers handed to WinHTTP
/// outlive the calls that use them.
fn winhttp_get(host: &str, path: &str) -> anyhow::Result<Response> {
    let agent = wide("audioviz");
    // Automatic proxy discovery honours the user's system proxy (Win 8.1+);
    // fall back to the static default-proxy mode if the OS refuses it.
    // SAFETY: the agent string is null-terminated and outlives both calls.
    let session = unsafe {
        let s = WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if s.is_null() {
            WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, PCWSTR::null(), PCWSTR::null(), 0)
        } else {
            s
        }
    };
    let session = WinHttpHandle::new(session, "WinHttpOpen")?;
    // SAFETY: `session` is a live session handle.
    unsafe { WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS) }
        .map_err(|e| anyhow!("WinHttpSetTimeouts failed: {}", describe_winhttp_error(&e)))?;

    let host_w = wide(host);
    // SAFETY: `session` is live and the host string is null-terminated.
    let connect = WinHttpHandle::new(unsafe { WinHttpConnect(session.0, PCWSTR(host_w.as_ptr()), 443, 0) }, "WinHttpConnect")?;

    let verb = wide("GET");
    let path_w = wide(path);
    // SAFETY: `connect` is live; verb/path are null-terminated; null version,
    // referrer and accept-types are documented defaults.
    let request = WinHttpHandle::new(
        unsafe {
            WinHttpOpenRequest(
                connect.0,
                PCWSTR(verb.as_ptr()),
                PCWSTR(path_w.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        },
        "WinHttpOpenRequest",
    )?;

    // WinHttpSendRequest takes the header block as a counted UTF-16 slice.
    let headers: Vec<u16> =
        "User-Agent: audioviz\r\nAccept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n".encode_utf16().collect();
    // SAFETY: `request` is live; the header slice outlives the call; no body.
    unsafe { WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0) }
        .map_err(|e| anyhow!("could not reach {host}: {}", describe_winhttp_error(&e)))?;
    // SAFETY: `request` is live; the reserved pointer must be null.
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|e| anyhow!("no response from {host}: {}", describe_winhttp_error(&e)))?;

    let mut status: u32 = 0;
    let mut len = std::mem::size_of::<u32>() as u32;
    // SAFETY: with WINHTTP_QUERY_FLAG_NUMBER the buffer is a u32 of `len` bytes;
    // a null name and null index are the documented "by level, no index" form.
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
    }
    .map_err(|e| anyhow!("could not read the HTTP status: {}", describe_winhttp_error(&e)))?;

    let mut bytes = Vec::new();
    loop {
        let mut available: u32 = 0;
        // SAFETY: `request` is live and `available` is a valid out-pointer.
        unsafe { WinHttpQueryDataAvailable(request.0, &mut available) }
            .map_err(|e| anyhow!("connection dropped while reading: {}", describe_winhttp_error(&e)))?;
        if available == 0 {
            break;
        }
        if bytes.len() + available as usize > MAX_BODY {
            bail!("response from {host} is larger than {MAX_BODY} bytes; refusing to read it");
        }
        let start = bytes.len();
        bytes.resize(start + available as usize, 0);
        let mut read: u32 = 0;
        // SAFETY: the destination slice has exactly `available` writable bytes.
        unsafe { WinHttpReadData(request.0, bytes[start..].as_mut_ptr().cast(), available, &mut read) }
            .map_err(|e| anyhow!("connection dropped while reading: {}", describe_winhttp_error(&e)))?;
        bytes.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }
    Ok(Response { status, bytes })
}

// ---- registry + diagnostics -------------------------------------------------

/// Open a registry key read-only; `None` if it does not exist.
fn reg_open(root: HKEY, subkey: &str) -> Option<RegKey> {
    let subkey_w = wide(subkey);
    let mut key = HKEY::default();
    // SAFETY: the subkey string is null-terminated and `key` is a valid out-pointer.
    let rc = unsafe { RegOpenKeyExW(root, PCWSTR(subkey_w.as_ptr()), None, KEY_READ, &mut key) };
    (rc == ERROR_SUCCESS).then_some(RegKey(key))
}

struct RegKey(HKEY);

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: the key was opened by RegOpenKeyExW and is closed once.
        let _: WIN32_ERROR = unsafe { RegCloseKey(self.0) };
    }
}

impl RegKey {
    /// Raw value bytes and type; `None` if the value is missing or oversized.
    fn query(&self, name: &str) -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
        let name_w = wide(name);
        let mut ty = REG_VALUE_TYPE::default();
        let mut buf = vec![0u8; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` has `len` writable bytes and every out-pointer is valid.
        let rc = unsafe {
            RegQueryValueExW(self.0, PCWSTR(name_w.as_ptr()), None, Some(&mut ty), Some(buf.as_mut_ptr()), Some(&mut len))
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(len as usize);
        Some((ty, buf))
    }

    fn string(&self, name: &str) -> Option<String> {
        let (ty, buf) = self.query(name)?;
        if ty != REG_SZ {
            return None;
        }
        let units: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }

    fn dword(&self, name: &str) -> Option<u32> {
        let (ty, buf) = self.query(name)?;
        (ty == REG_DWORD && buf.len() >= 4).then(|| u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
    }
}

/// One line naming the app version, Windows edition/build and machine
/// architecture, e.g. `audioviz 0.1.0 | Windows 11 Professional 24H2 build 26200.6901 | x64`.
/// The app appends GPU and capture details for bug reports.
pub fn diagnostics_header() -> String {
    let os = reg_open(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .map(|key| {
            let build: u32 = key.string("CurrentBuild").and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            // ProductName still says "Windows 10" on Windows 11; the build number is the truth.
            let mut s = if build >= 22000 { "Windows 11" } else { "Windows 10" }.to_owned();
            for part in [key.string("EditionID"), key.string("DisplayVersion")].into_iter().flatten() {
                if !part.is_empty() {
                    s.push(' ');
                    s.push_str(&part);
                }
            }
            match key.dword("UBR") {
                Some(ubr) => s.push_str(&format!(" build {build}.{ubr}")),
                None => s.push_str(&format!(" build {build}")),
            }
            s
        })
        .unwrap_or_else(|| "Windows (build unknown)".to_owned());

    let mut info = SYSTEM_INFO::default();
    // SAFETY: `info` is a valid, writable SYSTEM_INFO; the call cannot fail.
    unsafe { GetNativeSystemInfo(&mut info) };
    // SAFETY: GetNativeSystemInfo fills the processor-architecture member of the union.
    let arch = match unsafe { info.Anonymous.Anonymous.wProcessorArchitecture } {
        PROCESSOR_ARCHITECTURE_AMD64 => "x64".to_owned(),
        PROCESSOR_ARCHITECTURE_ARM64 => "arm64".to_owned(),
        PROCESSOR_ARCHITECTURE_INTEL => "x86".to_owned(),
        other => format!("arch {}", other.0),
    };

    format!("audioviz {} | {os} | {arch}", env!("CARGO_PKG_VERSION"))
}
