//! How full the system drive is.

use windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::HSTRING};

/// The share of the drive Windows is installed on that is in use, 0–100.
/// `None` if Windows will not say.
pub fn system_drive_used() -> Option<u8> {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned());
    let (mut free, mut total) = (0u64, 0u64);
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(format!("{drive}\\")), None, Some(&mut total), Some(&mut free)) }.ok()?;
    (total > 0).then(|| (100.0 * (1.0 - free as f64 / total as f64)).round().clamp(0.0, 100.0) as u8)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_system_drive_is_partly_full() {
        let used = super::system_drive_used().expect("Windows reports its own drive");
        assert!((1..=100).contains(&used), "{used}%");
    }
}
