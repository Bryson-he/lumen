//! The master volume of the default playback device: reading it, changing it
//! one step at a time, and hearing when anything changes it. All through the
//! public Core Audio API, in this process only.

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RPC_E_CHANGED_MODE, WPARAM},
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA,
            Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl},
            IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia, eRender,
        },
        System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree},
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::{GUID, implement},
};

/// One press of a volume key moves the volume this much, as in Windows.
const STEP: f32 = 0.02;

/// The volume as a flyout shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    /// 0–100.
    pub percent: u8,
    pub muted: bool,
}

/// Posts a message to a window whenever the device's volume or mute changes.
/// Windows calls this on one of its own audio threads.
#[implement(IAudioEndpointVolumeCallback)]
struct Notifier {
    window: isize,
    message: u32,
}

impl IAudioEndpointVolumeCallback_Impl for Notifier_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.window as *mut _)), self.message, WPARAM(0), LPARAM(0));
        }
        Ok(())
    }
}

struct Endpoint {
    id: String,
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe {
            let _ = self.volume.UnregisterControlChangeNotify(&self.callback);
        }
    }
}

/// The default playback device's volume. Lives on the thread that made it.
pub struct Volume {
    enumerator: IMMDeviceEnumerator,
    endpoint: Option<Endpoint>,
    window: isize,
    message: u32,
}

impl Volume {
    /// `message` is posted to `window` whenever the volume or mute changes,
    /// whoever changed it.
    pub fn new(window: isize, message: u32) -> Option<Self> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() && hr != RPC_E_CHANGED_MODE {
                return None;
            }
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let mut volume = Self { enumerator, endpoint: None, window, message };
            volume.refresh();
            Some(volume)
        }
    }

    /// Follows the default device if it has changed (headphones plugged in,
    /// another output chosen). `None` if there is no playback device.
    fn refresh(&mut self) -> Option<&Endpoint> {
        unsafe {
            let Ok(device) = self.enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) else {
                self.endpoint = None;
                return None;
            };
            let id = device.GetId().ok().map(|raw| {
                let id = raw.to_string().unwrap_or_default();
                CoTaskMemFree(Some(raw.0 as *const _));
                id
            })?;
            if self.endpoint.as_ref().is_none_or(|endpoint| endpoint.id != id) {
                self.endpoint = None;
                let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
                let callback: IAudioEndpointVolumeCallback = Notifier { window: self.window, message: self.message }.into();
                let _ = volume.RegisterControlChangeNotify(&callback);
                self.endpoint = Some(Endpoint { id, volume, callback });
            }
            self.endpoint.as_ref()
        }
    }

    pub fn level(&mut self) -> Option<Level> {
        let endpoint = self.refresh()?;
        unsafe {
            let scalar = endpoint.volume.GetMasterVolumeLevelScalar().ok()?;
            let muted = endpoint.volume.GetMute().ok()?.as_bool();
            Some(Level { percent: percent(scalar), muted })
        }
    }

    /// One step up or down. Turning the volume up also unmutes, as in Windows.
    pub fn step(&mut self, up: bool) -> Option<Level> {
        let endpoint = self.refresh()?;
        unsafe {
            let now = endpoint.volume.GetMasterVolumeLevelScalar().ok()?;
            let _ = endpoint.volume.SetMasterVolumeLevelScalar(stepped(now, up), &GUID::zeroed());
            if up {
                let _ = endpoint.volume.SetMute(false, &GUID::zeroed());
            }
        }
        self.level()
    }

    pub fn toggle_mute(&mut self) -> Option<Level> {
        let endpoint = self.refresh()?;
        unsafe {
            let muted = endpoint.volume.GetMute().ok()?.as_bool();
            let _ = endpoint.volume.SetMute(!muted, &GUID::zeroed());
        }
        self.level()
    }
}

fn percent(scalar: f32) -> u8 {
    (scalar.clamp(0.0, 1.0) * 100.0).round() as u8
}

/// The level one step from `now`, kept on whole steps so repeated presses
/// land on 0, 2, 4, … 100 rather than drifting.
fn stepped(now: f32, up: bool) -> f32 {
    let steps = (now / STEP).round() + if up { 1.0 } else { -1.0 };
    (steps * STEP).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stay_on_the_grid_and_inside_the_range() {
        assert!((stepped(0.50, true) - 0.52).abs() < 1e-5);
        assert!((stepped(0.50, false) - 0.48).abs() < 1e-5);
        assert!((stepped(0.507, true) - 0.52).abs() < 1e-5, "an odd level snaps to the next step");
        assert_eq!(stepped(1.0, true), 1.0);
        assert_eq!(stepped(0.0, false), 0.0);
        assert_eq!(stepped(0.01, false), 0.0);
    }

    #[test]
    fn percentages_round_to_the_nearest_whole_number() {
        assert_eq!(percent(0.0), 0);
        assert_eq!(percent(0.499), 50);
        assert_eq!(percent(1.2), 100);
    }

    #[test]
    fn reads_the_volume_when_there_is_a_playback_device() {
        // No window to notify: the message is never delivered.
        let Some(mut volume) = Volume::new(0, 0) else { return };
        if let Some(level) = volume.level() {
            assert!(level.percent <= 100);
        }
    }
}
