//! The now-playing widget, fed by Windows' media session (the same source as
//! the media keys overlay), so it works with any player that reports there.
//!
//! One listener thread serves every player widget. It sleeps until Windows
//! says something changed; there is no polling.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use lumen_render::{Bitmap, Canvas, Rect, Weight};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Storage::Streams::{DataReader, IRandomAccessStreamReference},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize},
};

use crate::paint::Paint;

/// Default size in logical pixels.
pub const SIZE: (f32, f32) = (320.0, 96.0);

/// Album art is kept at this size (pixels per side); the widget shows it at 72.
const ART_SIZE: u32 = 144;

/// What is playing right now.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// Position when this was read, and the track length (zero if unknown).
    pub position: Duration,
    pub length: Duration,
    pub read_at: Instant,
    /// Album art as `ART_SIZE`-square RGBA pixels.
    pub art: Option<Art>,
}

#[derive(Clone, PartialEq)]
pub struct Art {
    pub size: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Art {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Art({0}x{0})", self.size)
    }
}

impl MediaInfo {
    /// How far through the track we are now, 0.0–1.0. `None` if the length is unknown.
    pub fn progress(&self) -> Option<f32> {
        if self.length.is_zero() {
            return None;
        }
        let elapsed = if self.playing { self.read_at.elapsed() } else { Duration::ZERO };
        Some(((self.position + elapsed).as_secs_f32() / self.length.as_secs_f32()).clamp(0.0, 1.0))
    }

    /// Whether the progress bar is moving and needs regular redraws.
    pub fn is_advancing(&self) -> bool {
        self.playing && !self.length.is_zero()
    }
}

static PAUSED: AtomicBool = AtomicBool::new(false);
/// Wakes the listener thread when it is resumed.
static RESUME: Mutex<Option<mpsc::Sender<()>>> = Mutex::new(None);

/// Stops (or restarts) all work in the listener. While paused it ignores
/// what Windows tells it; on resume it reads the current state again.
pub fn set_paused(paused: bool) {
    if PAUSED.swap(paused, Ordering::Relaxed) != paused
        && !paused
        && let Some(wake) = RESUME.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
    {
        let _ = wake.send(());
    }
}

/// Plays or pauses whatever Windows' media session is playing, like the
/// keyboard's play/pause key. Done on a short-lived thread so the caller never waits.
pub fn toggle_play_pause() {
    let _ = std::thread::Builder::new().name("media-toggle".into()).spawn(|| {
        let toggle = || -> windows::core::Result<()> {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.ok();
            Manager::RequestAsync()?.join()?.GetCurrentSession()?.TryTogglePlayPauseAsync()?.join()?;
            Ok(())
        };
        if let Err(error) = toggle() {
            tracing::debug!("nothing to play or pause: {error}");
        }
    });
}

/// Starts the listener thread. `on_change` is called (on that thread) with the
/// current track whenever it changes, and with `None` when nothing is playing.
pub fn watch(on_change: impl Fn(Option<MediaInfo>) + Send + 'static) {
    let spawned = std::thread::Builder::new().name("media".into()).spawn(move || {
        if let Err(error) = listen(&on_change) {
            tracing::warn!("now playing is unavailable: {error}");
            on_change(None);
        }
    });
    if let Err(error) = spawned {
        tracing::warn!("could not start the media listener: {error}");
    }
}

fn listen(on_change: &dyn Fn(Option<MediaInfo>)) -> windows::core::Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.ok();
    let manager = Manager::RequestAsync()?.join()?;

    // Every Windows event just wakes this thread, which then re-reads the state.
    let (wake, woken) = mpsc::channel::<()>();
    let notify = wake.clone();
    manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = notify.send(());
        Ok(())
    }))?;

    *RESUME.lock().unwrap_or_else(|p| p.into_inner()) = Some(wake.clone());

    let mut subscribed: Option<Session> = None;
    let mut last: Option<MediaInfo> = None;
    let mut first = true;
    loop {
        // While paused (a game is running), events are swallowed: no reading
        // of titles, no decoding of cover art. Resuming sends one wake-up, so
        // the state is read fresh then.
        while PAUSED.load(Ordering::Relaxed) {
            if woken.recv().is_err() {
                return Ok(());
            }
        }
        let session = manager.GetCurrentSession().ok();

        // Follow the current session's own events.
        if session != subscribed {
            if let Some(session) = &session {
                let (a, b, c) = (wake.clone(), wake.clone(), wake.clone());
                let _ = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = a.send(());
                    Ok(())
                }));
                let _ = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = b.send(());
                    Ok(())
                }));
                let _ = session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = c.send(());
                    Ok(())
                }));
            }
            subscribed = session.clone();
        }

        let info = session.as_ref().and_then(|session| read(session, last.as_ref()));
        if first || !same_for_display(info.as_ref(), last.as_ref()) {
            on_change(info.clone());
        }
        last = info;
        first = false;

        // Sleep until Windows reports a change, then let a burst of events settle.
        if woken.recv().is_err() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(80));
        while woken.try_recv().is_ok() {}
    }
}

/// Timeline events arrive every few seconds while playing; they are only worth
/// a redraw if the position jumped (a seek) rather than simply advancing.
fn same_for_display(a: Option<&MediaInfo>, b: Option<&MediaInfo>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let expected = b.position + if b.playing { a.read_at.duration_since(b.read_at) } else { Duration::ZERO };
            let drift = a.position.abs_diff(expected);
            a.title == b.title && a.artist == b.artist && a.playing == b.playing && a.length == b.length && a.art.is_some() == b.art.is_some() && drift < Duration::from_secs(3)
        }
        _ => false,
    }
}

fn read(session: &Session, previous: Option<&MediaInfo>) -> Option<MediaInfo> {
    let properties = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = properties.Title().map(|s| s.to_string()).unwrap_or_default();
    let artist = properties.Artist().map(|s| s.to_string()).unwrap_or_default();
    if title.is_empty() && artist.is_empty() {
        return None;
    }

    let playing = session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).is_ok_and(|status| status == PlaybackStatus::Playing);
    let (position, length) = session
        .GetTimelineProperties()
        .ok()
        .map(|timeline| {
            let ticks = |value: windows::core::Result<windows::Foundation::TimeSpan>| value.map_or(0, |t| t.Duration.max(0)) as u64;
            let start = ticks(timeline.StartTime());
            // TimeSpan counts 100-nanosecond steps.
            (Duration::from_nanos(ticks(timeline.Position()).saturating_sub(start) * 100), Duration::from_nanos(ticks(timeline.EndTime()).saturating_sub(start) * 100))
        })
        .unwrap_or_default();

    // The same track keeps its art; decoding again would be wasted work.
    let art = match previous {
        Some(previous) if previous.title == title && previous.artist == artist && previous.art.is_some() => previous.art.clone(),
        _ => properties.Thumbnail().ok().and_then(|thumbnail| read_art(&thumbnail)),
    };

    Some(MediaInfo { title, artist, playing, position, length, read_at: Instant::now(), art })
}

fn read_art(thumbnail: &IRandomAccessStreamReference) -> Option<Art> {
    let stream = thumbnail.OpenReadAsync().ok()?.join().ok()?;
    let size = u32::try_from(stream.Size().ok()?).ok().filter(|size| *size > 0 && *size < 16 * 1024 * 1024)?;
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut encoded = vec![0u8; size as usize];
    reader.ReadBytes(&mut encoded).ok()?;

    let image = image::load_from_memory(&encoded).ok()?;
    let square = image.resize_to_fill(ART_SIZE, ART_SIZE, image::imageops::FilterType::Triangle).into_rgba8();
    Some(Art { size: ART_SIZE, rgba: square.into_raw() })
}

pub fn draw(canvas: &Canvas<'_>, paint: &Paint, info: Option<&MediaInfo>, art: Option<&Bitmap>, width: f32, height: f32) {
    paint.card(canvas, width, height);
    let art_size = height - 24.0;
    let art_rect = Rect::new(12.0, 12.0, art_size, art_size);
    let art_radius = paint.radius.min(8.0);
    match art {
        Some(bitmap) => canvas.image(bitmap, art_rect, art_radius),
        // No cover: a soft gradient in the theme's colours.
        None => canvas.fill_round_rect_gradient(art_rect, art_radius, paint.primary, paint.tertiary),
    }

    let x = 12.0 + art_size + 12.0;
    let text_width = width - x - 14.0;
    match info {
        Some(info) => {
            canvas.text(&info.title, Rect::new(x, 15.0, text_width, 20.0), paint.body(15.0, Weight::SemiBold, paint.text));
            canvas.text(&info.artist, Rect::new(x, 38.0, text_width, 18.0), paint.body(13.0, Weight::Regular, paint.text_muted));
            if let Some(progress) = info.progress() {
                paint.bar(canvas, Rect::new(x, height - 12.0 - 5.0 - 6.0, text_width, 5.0), progress, paint.primary);
            }
        }
        None => {
            canvas.text("Nothing playing", Rect::new(x, 26.0, text_width, 20.0), paint.body(15.0, Weight::SemiBold, paint.text_muted));
            canvas.text("Start some music or a video", Rect::new(x, 49.0, text_width, 18.0), paint.body(13.0, Weight::Regular, paint.text_muted));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(position: u64, length: u64, playing: bool) -> MediaInfo {
        MediaInfo {
            title: "Track".into(),
            artist: "Artist".into(),
            playing,
            position: Duration::from_secs(position),
            length: Duration::from_secs(length),
            read_at: Instant::now(),
            art: None,
        }
    }

    #[test]
    fn progress_is_position_over_length() {
        let paused = info(60, 240, false);
        assert!((paused.progress().unwrap() - 0.25).abs() < 0.001);
        assert!(!paused.is_advancing());
        assert_eq!(info(300, 240, false).progress(), Some(1.0), "never past the end");
        assert_eq!(info(10, 0, true).progress(), None, "unknown length has no bar");
        assert!(!info(10, 0, true).is_advancing());
    }

    #[test]
    fn a_playing_track_advances_with_the_clock() {
        let mut playing = info(60, 240, true);
        playing.read_at = Instant::now() - Duration::from_secs(60);
        assert!((playing.progress().unwrap() - 0.5).abs() < 0.01);
        assert!(playing.is_advancing());
    }

    #[test]
    fn steady_playback_is_not_a_change_but_a_seek_is() {
        let before = info(60, 240, true);
        let mut later = info(65, 240, true);
        later.read_at = before.read_at + Duration::from_secs(5);
        assert!(same_for_display(Some(&later), Some(&before)), "5 s on, 5 s later: nothing new to draw");

        let mut seeked = info(180, 240, true);
        seeked.read_at = before.read_at + Duration::from_secs(5);
        assert!(!same_for_display(Some(&seeked), Some(&before)));

        let mut paused = later.clone();
        paused.playing = false;
        assert!(!same_for_display(Some(&paused), Some(&before)));
        assert!(!same_for_display(None, Some(&before)));
        assert!(same_for_display(None, None));
    }
}
