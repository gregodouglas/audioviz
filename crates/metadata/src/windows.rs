//! Windows System Media Transport Controls (SMTC) backend. Polls once a
//! second; fetches and decodes the thumbnail only when the track changes.
//! Transport commands arrive on a channel and are executed between polls,
//! followed by an immediate re-poll so the UI sees the result quickly.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use tracing::{info, warn};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

use crate::{MediaCommand, MetadataHandle, MetadataSource, NowPlaying, Shared, decode_art, track_id};

pub struct SmtcSource;

impl MetadataSource for SmtcSource {
    fn start(&self, app_filter: &str) -> anyhow::Result<MetadataHandle> {
        let (handle, shared) = MetadataHandle::new();
        let filter = app_filter.to_lowercase();
        std::thread::Builder::new()
            .name("metadata-smtc".into())
            .spawn(move || {
                if let Err(e) = poll_loop(&filter, &shared) {
                    warn!("metadata thread ended: {e:#}");
                }
            })?;
        Ok(handle)
    }
}

/// Prefer a session whose app id contains `filter`; otherwise the system's
/// current session. Returns the session with its lowercased app id and
/// whether it matched the filter.
fn pick_session(manager: &Manager, filter: &str) -> Option<(Session, String, bool)> {
    if let Ok(sessions) = manager.GetSessions() {
        for s in sessions.into_iter() {
            if let Ok(id) = s.SourceAppUserModelId() {
                let id = id.to_string().to_lowercase();
                if id.contains(filter) {
                    return Some((s, id, true));
                }
            }
        }
    }
    let s = manager.GetCurrentSession().ok()?;
    let id = s.SourceAppUserModelId().map(|id| id.to_string().to_lowercase()).unwrap_or_default();
    let is_target = !filter.is_empty() && id.contains(filter);
    Some((s, id, is_target))
}

fn read_thumbnail(session: &Session) -> Option<Vec<u8>> {
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let thumb = props.Thumbnail().ok()?;
    let stream = thumb.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 16 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(bytes)
}

/// Run a transport command against `session`, logging (not propagating) failures.
fn run_command(session: &Session, cmd: MediaCommand) {
    let result = match cmd {
        MediaCommand::TogglePlayPause => session.TryTogglePlayPauseAsync().and_then(|op| op.join()),
        MediaCommand::Next => session.TrySkipNextAsync().and_then(|op| op.join()),
        MediaCommand::Previous => session.TrySkipPreviousAsync().and_then(|op| op.join()),
    };
    match result {
        Ok(true) => info!("media command {cmd:?} accepted"),
        Ok(false) => warn!("media command {cmd:?} rejected by session"),
        Err(e) => warn!("media command {cmd:?} failed: {e}"),
    }
}

fn poll_loop(filter: &str, shared: &Shared) -> anyhow::Result<()> {
    let manager = Manager::RequestAsync()?.join()?;
    let mut last_id: Option<u64> = None;
    let mut art = None;
    let mut session: Option<Session> = None;
    while !shared.stop.load(Ordering::Relaxed) {
        let np = pick_session(&manager, filter).and_then(|(sess, source_app, is_target)| {
            let props = sess.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
            let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
            let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
            let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
            let id = track_id(&title, &artist, &album);
            if last_id != Some(id) {
                last_id = Some(id);
                art = read_thumbnail(&sess).and_then(|b| decode_art(&b)).map(Arc::new);
                info!("now playing: {artist} — {title} [{album}] art={}", art.is_some());
            }
            let playing = sess.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()).map(|s| s == Status::Playing).unwrap_or(false);
            let (position_s, duration_s) = sess
                .GetTimelineProperties()
                .ok()
                .map(|t| {
                    let pos = t.Position().map(|d| d.Duration as f64 / 1e7).unwrap_or(0.0);
                    let end = t.EndTime().map(|d| d.Duration as f64 / 1e7).unwrap_or(0.0);
                    (pos, end)
                })
                .unwrap_or((0.0, 0.0));
            session = Some(sess);
            Some(NowPlaying {
                title,
                artist,
                album,
                playing,
                position_s,
                duration_s,
                art: art.clone(),
                track_id: id,
                updated: Instant::now(),
                source_app,
                is_target,
            })
        });
        if np.is_none() {
            session = None;
        }
        if let Ok(mut slot) = shared.latest.lock() {
            *slot = np;
        }

        // Wait for the next poll, or wake early on a command. Drain anything
        // queued so a burst of clicks is applied before the re-poll.
        match shared.commands.recv_timeout(Duration::from_millis(1000)) {
            Ok(cmd) => {
                let queued = std::iter::once(cmd).chain(shared.commands.try_iter());
                for cmd in queued {
                    match &session {
                        Some(s) => run_command(s, cmd),
                        None => warn!("media command {cmd:?} ignored: no session"),
                    }
                }
                // Give the player a moment to update its status before re-polling.
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(RecvTimeoutError::Timeout) => {}
            // Handle dropped; `stop` is set in its Drop, so the loop ends.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}
