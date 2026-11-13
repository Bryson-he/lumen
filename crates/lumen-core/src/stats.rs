//! The core's own memory and CPU use, for the Home status strip.

use std::time::{Duration, Instant};

use windows::Win32::{
    Foundation::FILETIME,
    System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{GetCurrentProcess, GetProcessTimes},
    },
};

/// Samples this process. CPU is measured between one sample and the next.
pub struct ProcessStats {
    last_cpu: Duration,
    last_at: Instant,
    last_percent: f64,
    cores: f64,
}

impl ProcessStats {
    pub fn new() -> Self {
        Self {
            last_cpu: cpu_time(),
            last_at: Instant::now(),
            last_percent: 0.0,
            cores: std::thread::available_parallelism().map_or(1.0, |n| n.get() as f64),
        }
    }

    /// Returns `(memory in MB, percent of the whole CPU)`, like Task Manager shows.
    pub fn sample(&mut self) -> (f64, f64) {
        let elapsed = self.last_at.elapsed();
        // Too short a window gives jumpy numbers; reuse the last reading.
        if elapsed >= Duration::from_millis(250) {
            let cpu = cpu_time();
            let used = cpu.saturating_sub(self.last_cpu);
            self.last_percent = (used.as_secs_f64() / elapsed.as_secs_f64() / self.cores * 100.0).clamp(0.0, 100.0);
            self.last_cpu = cpu;
            self.last_at = Instant::now();
        }
        (memory_mb(), self.last_percent)
    }
}

/// Total CPU time (kernel + user) this process has used.
fn cpu_time() -> Duration {
    let (mut created, mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) }.is_err() {
        return Duration::ZERO;
    }
    // FILETIME counts 100-nanosecond steps.
    let ticks = |t: FILETIME| u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime);
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

/// Working set: the physical memory this process is using right now.
fn memory_mb() -> f64 {
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    if unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) }.as_bool() {
        counters.WorkingSetSize as f64 / (1024.0 * 1024.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_are_plausible() {
        let mut stats = ProcessStats::new();
        // Burn a little CPU so there is something to measure.
        let mut x = 0u64;
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            x = x.wrapping_mul(31).wrapping_add(7);
        }
        std::hint::black_box(x);
        let (memory, cpu) = stats.sample();
        assert!(memory > 1.0 && memory < 4096.0, "memory {memory} MB");
        assert!(cpu > 0.0 && cpu <= 100.0, "cpu {cpu}%");
    }
}
