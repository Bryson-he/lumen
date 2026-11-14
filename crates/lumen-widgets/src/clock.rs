//! The clock widget: the date and the time, redrawn once a minute.

use windows::{
    Win32::{
        Foundation::SYSTEMTIME,
        Globalization::{GetDateFormatEx, GetLocaleInfoEx, GetTimeFormatEx, LOCALE_ITIME, TIME_FORMAT_FLAGS},
        System::SystemInformation::GetLocalTime,
    },
    core::{PCWSTR, w},
};


/// Default size in logical pixels.
pub const SIZE: (f32, f32) = (190.0, 92.0);

/// What the clock shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClockData {
    /// "Thursday 13 Nov", in the user's language.
    pub date: String,
    /// "10:42", in 12- or 24-hour form as Windows is set.
    pub time: String,
    /// "AM" / "PM" in 12-hour form, otherwise empty.
    pub suffix: String,
    /// The pieces of the date and time on their own, for laying out separately:
    /// "Saturday", "14", "November", "2026", "5" (or "17"), "21".
    pub weekday: String,
    pub day: String,
    pub month: String,
    pub year: String,
    pub hour: String,
    pub minute: String,
}

impl ClockData {
    /// The current local date and time, formatted the way the user's Windows is set up.
    pub fn now() -> Self {
        let now = unsafe { GetLocalTime() };
        let twelve_hour = locale_info(LOCALE_ITIME).starts_with('0');
        Self {
            date: format_date(&now, w!("dddd d MMM")),
            time: format_time(&now, if twelve_hour { w!("h:mm") } else { w!("H:mm") }),
            suffix: if twelve_hour { format_time(&now, w!("tt")) } else { String::new() },
            weekday: format_date(&now, w!("dddd")),
            day: format_date(&now, w!("d ")).trim().to_owned(),
            month: format_date(&now, w!("MMMM")),
            year: format_date(&now, w!("yyyy")),
            hour: format_time(&now, if twelve_hour { w!("h ") } else { w!("H ") }).trim().to_owned(),
            minute: format_time(&now, w!("mm")),
        }
    }
}

/// Milliseconds until the next minute starts, when the clock next needs drawing.
pub fn millis_until_next_minute() -> u32 {
    let now = unsafe { GetLocalTime() };
    let into_minute = u32::from(now.wSecond) * 1000 + u32::from(now.wMilliseconds);
    // A little past the boundary, so the new minute is certainly showing.
    60_000u32.saturating_sub(into_minute) + 30
}

fn format_date(time: &SYSTEMTIME, format: PCWSTR) -> String {
    let mut buffer = [0u16; 80];
    let len = unsafe { GetDateFormatEx(PCWSTR::null(), Default::default(), Some(time), format, Some(&mut buffer), PCWSTR::null()) };
    text(&buffer, len)
}

fn format_time(time: &SYSTEMTIME, format: PCWSTR) -> String {
    let mut buffer = [0u16; 80];
    let len = unsafe { GetTimeFormatEx(PCWSTR::null(), TIME_FORMAT_FLAGS(0), Some(time), format, Some(&mut buffer)) };
    text(&buffer, len)
}

fn locale_info(kind: u32) -> String {
    let mut buffer = [0u16; 16];
    let len = unsafe { GetLocaleInfoEx(PCWSTR::null(), kind, Some(&mut buffer)) };
    text(&buffer, len)
}

/// The Windows calls return the length including the ending zero.
fn text(buffer: &[u16], len: i32) -> String {
    String::from_utf16_lossy(&buffer[..(len.max(1) - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_formatted() {
        let clock = ClockData::now();
        assert!(clock.time.contains(':'), "time {:?}", clock.time);
        assert!(clock.time.len() >= 4 && clock.time.len() <= 5, "time {:?}", clock.time);
        assert!(clock.date.chars().any(|c| c.is_ascii_digit()), "date {:?}", clock.date);
        assert!(clock.date.split_whitespace().count() >= 3, "date {:?}", clock.date);
    }

    #[test]
    fn next_redraw_is_within_a_minute() {
        let wait = millis_until_next_minute();
        assert!((30..=60_030).contains(&wait), "{wait} ms");
    }
}
