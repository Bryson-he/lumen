//! The system stats widget: CPU, memory and GPU use.
//!
//! One [`StatsSampler`] feeds every stats widget. It asks Windows directly
//! (processor times, memory status, and the same GPU performance counter Task
//! Manager reads), which costs far less than a general system-info library.

use lumen_render::{Canvas, Rect, Weight};
use windows::{
    Win32::{
        Foundation::FILETIME,
        System::{
            Performance::{
                PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
                PdhGetFormattedCounterArrayW, PdhOpenQueryW,
            },
            SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::GetSystemTimes,
        },
    },
    core::w,
};

use crate::paint::Paint;

/// Default size in logical pixels.
pub const SIZE: (f32, f32) = (190.0, 150.0);

/// How often the sampler should be asked, in milliseconds.
pub const SAMPLE_INTERVAL_MS: u32 = 2000;

/// What the widget shows: whole percentages, so it redraws only on a visible change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatsData {
    pub cpu: u8,
    pub memory: u8,
    /// `None` when Windows offers no GPU counter (some VMs and remote sessions).
    pub gpu: Option<u8>,
}

pub struct StatsSampler {
    last_idle: u64,
    last_total: u64,
    gpu: Option<GpuCounter>,
    last_gpu: Option<u8>,
}

impl StatsSampler {
    pub fn new() -> Self {
        let (last_idle, last_total) = processor_times();
        Self { last_idle, last_total, gpu: GpuCounter::open(), last_gpu: None }
    }

    /// Reads the current values. CPU and GPU are averages since the previous call.
    pub fn sample(&mut self) -> StatsData {
        let (idle, total) = processor_times();
        let (idle_delta, total_delta) = (idle.saturating_sub(self.last_idle), total.saturating_sub(self.last_total));
        (self.last_idle, self.last_total) = (idle, total);
        let cpu = if total_delta == 0 { 0.0 } else { 100.0 * (1.0 - idle_delta as f64 / total_delta as f64) };

        // A GPU reading can fail now and then (two samples too close together,
        // or the list of engines changing mid-read); keep the last good one.
        if let Some(reading) = self.gpu.as_mut().and_then(GpuCounter::read) {
            self.last_gpu = Some(percent(reading));
        }
        StatsData { cpu: percent(cpu), memory: memory_load(), gpu: self.last_gpu }
    }
}

impl Default for StatsSampler {
    fn default() -> Self {
        Self::new()
    }
}

fn percent(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}

/// `(idle, total)` processor time across all cores, in 100 ns steps.
fn processor_times() -> (u64, u64) {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_err() {
        return (0, 0);
    }
    let ticks = |t: FILETIME| u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime);
    // Kernel time already includes idle time.
    (ticks(idle), ticks(kernel) + ticks(user))
}

/// Percentage of physical memory in use.
fn memory_load() -> u8 {
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_ok() { status.dwMemoryLoad.min(100) as u8 } else { 0 }
}

/// The "GPU Engine … Utilization Percentage" counter for 3D work, summed over
/// every process: the figure Task Manager shows as GPU "3D".
struct GpuCounter {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    buffer: Vec<u8>,
}

impl GpuCounter {
    fn open() -> Option<Self> {
        unsafe {
            let mut query = PDH_HQUERY::default();
            if PdhOpenQueryW(None, 0, &mut query) != 0 {
                return None;
            }
            let mut counter = PDH_HCOUNTER::default();
            if PdhAddEnglishCounterW(query, w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"), 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            // Rate counters need two collections; this is the first.
            PdhCollectQueryData(query);
            Some(Self { query, counter, buffer: Vec::new() })
        }
    }

    fn read(&mut self) -> Option<f64> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            // First call reports the size needed; the list of engines changes as apps start and stop.
            let (mut size, mut count) = (0u32, 0u32);
            PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if size == 0 {
                return Some(0.0);
            }
            self.buffer.resize(size as usize, 0);
            let items = self.buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                return None;
            }
            let total: f64 = (0..count as usize).map(|i| (*items.add(i)).FmtValue.Anonymous.doubleValue).filter(|v| v.is_finite()).sum();
            Some(total)
        }
    }
}

impl Drop for GpuCounter {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

pub fn draw(canvas: &Canvas<'_>, paint: &Paint, data: &StatsData, width: f32, height: f32) {
    paint.card(canvas, width, height);
    let inner = width - 32.0;
    canvas.text("System", Rect::new(16.0, 13.0, inner, 18.0), paint.body(13.0, Weight::SemiBold, paint.text));

    let rows = [("CPU", Some(data.cpu), paint.primary), ("RAM", Some(data.memory), paint.tertiary), ("GPU", data.gpu, paint.accent)];
    for (index, (label, value, colour)) in rows.into_iter().enumerate() {
        let y = 40.0 + index as f32 * 35.0;
        canvas.text(label, Rect::new(16.0, y, inner, 18.0), paint.body(13.0, Weight::Regular, paint.text));
        let mut number = paint.body(13.0, Weight::Regular, paint.text_muted);
        number.align_right = true;
        let shown = value.map_or("n/a".to_owned(), |v| format!("{v}%"));
        canvas.text(&shown, Rect::new(16.0, y, inner, 18.0), number);
        paint.bar(canvas, Rect::new(16.0, y + 21.0, inner, 5.0), f32::from(value.unwrap_or(0)) / 100.0, colour);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn samples_are_percentages() {
        let mut sampler = StatsSampler::new();
        std::thread::sleep(Duration::from_millis(300));
        let data = sampler.sample();
        assert!(data.cpu <= 100);
        assert!(data.memory > 0 && data.memory <= 100, "memory {}", data.memory);
        if let Some(gpu) = data.gpu {
            assert!(gpu <= 100);
        }
    }

    #[test]
    fn percent_rounds_and_clamps() {
        assert_eq!(percent(12.4), 12);
        assert_eq!(percent(99.6), 100);
        assert_eq!(percent(250.0), 100);
        assert_eq!(percent(-3.0), 0);
    }
}
