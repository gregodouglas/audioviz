//! Windows System Media Transport Controls (SMTC) backend. Polls once a
//! second; fetches and decodes the thumbnail only when the track changes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::{info, warn};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

use crate::{MetadataHandle, MetadataSource, NowPlaying, decode_art, track_id};

pub struct SmtcSource;

impl MetadataSource for SmtcSource {
    fn start(&self, app_filter: &str) -> anyhow::Result<MetadataHandle> {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let filter = app_filter.to_lowercase();
        let (l2, s2) = (latest.clone(), stop.clone());
        std::thread::Builder::new()
            .name("metadata-smtc".into())
            .spawn(move || {
                if let Err(e) = poll_loop(&filter, &l2, &s2) {
                    warn!("metadata thread ended: {e:#}");
                }
            })?;
        Ok(MetadataHandle { latest, stop })
    }
}

fn pick_session(manager: &Manager, filter: &str) -> Option<Session> {
    if let Ok(sessions) = manager.GetSessions() {
        for s in sessions.into_iter() {
            if let Ok(id) = s.SourceAppUserModelId() {
                if id.to_string().to_lowercase().contains(filter) {
                    return Some(s);
                }
            }
        }
    }
    manager.GetCurrentSession().ok()
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

fn poll_loop(filter: &str, latest: &Mutex<Option<NowPlaying>>, stop: &AtomicBool) -> anyhow::Result<()> {
    let manager = Manager::RequestAsync()?.join()?;
    let mut last_id: Option<u64> = None;
    let mut art = None;
    while !stop.load(Ordering::Relaxed) {
        let np = pick_session(&manager, filter).and_then(|session| {
            let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
            let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
            let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
            let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
            let id = track_id(&title, &artist, &album);
            if last_id != Some(id) {
                last_id = Some(id);
                art = read_thumbnail(&session).and_then(|b| decode_art(&b)).map(Arc::new);
                info!("now playing: {artist} — {title} [{album}] art={}", art.is_some());
            }
            let playing = session.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()).map(|s| s == Status::Playing).unwrap_or(false);
            let (position_s, duration_s) = session
                .GetTimelineProperties()
                .ok()
                .map(|t| {
                    let pos = t.Position().map(|d| d.Duration as f64 / 1e7).unwrap_or(0.0);
                    let end = t.EndTime().map(|d| d.Duration as f64 / 1e7).unwrap_or(0.0);
                    (pos, end)
                })
                .unwrap_or((0.0, 0.0));
            Some(NowPlaying { title, artist, album, playing, position_s, duration_s, art: art.clone(), track_id: id, updated: Instant::now() })
        });
        if let Ok(mut slot) = latest.lock() {
            *slot = np;
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    Ok(())
}
