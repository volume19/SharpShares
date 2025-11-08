//! Progress tracking and status updates during enumeration

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tokio::time;

/// Tracks progress of share enumeration across threads
#[derive(Debug)]
pub struct Status {
    /// Count at last status print
    pub last_count: Arc<AtomicUsize>,
    /// Current number of hosts completed
    pub current_count: Arc<AtomicUsize>,
    /// Total number of hosts to process
    pub total_count: Arc<AtomicUsize>,
    /// Start time for rate calculation
    start_time: Instant,
}

impl Status {
    /// Create a new status tracker
    pub fn new(total: usize) -> Self {
        Self {
            last_count: Arc::new(AtomicUsize::new(0)),
            current_count: Arc::new(AtomicUsize::new(0)),
            total_count: Arc::new(AtomicUsize::new(total)),
            start_time: Instant::now(),
        }
    }

    /// Start periodic status update timer (every 30 seconds)
    /// Returns a JoinHandle that can be awaited or cancelled
    pub fn start_timer(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(30));
            // Skip the first tick (immediate)
            interval.tick().await;

            loop {
                interval.tick().await;
                self.print_status();
                // Update last_count for next iteration
                let current = self.current_count.load(Ordering::Relaxed);
                self.last_count.store(current, Ordering::Relaxed);
            }
        })
    }

    /// Increment the current count (thread-safe)
    pub fn increment(&self) {
        self.current_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Print current status to stderr
    pub fn print_status(&self) {
        let current = self.current_count.load(Ordering::Relaxed);
        let last = self.last_count.load(Ordering::Relaxed);
        let total = self.total_count.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed();

        let percentage = if total > 0 {
            (current as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let delta = current.saturating_sub(last);
        let rate = if elapsed.as_secs() > 0 {
            current as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        // Get memory usage (cross-platform via sysinfo or manual)
        let mem_mb = get_memory_usage_mb();

        eprintln!(
            "Status: ({:.2}%) {} computers finished (+{} {:.2}/s) -- Using {} MB RAM",
            percentage, current, delta, rate, mem_mb
        );
    }

    /// Get final summary
    pub fn print_final(&self) {
        let current = self.current_count.load(Ordering::Relaxed);
        let total = self.total_count.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed();

        tracing::info!(
            "Enumeration complete: {}/{} hosts in {:.2}s",
            current,
            total,
            elapsed.as_secs_f64()
        );
    }
}

/// Get current process memory usage in MB (cross-platform)
fn get_memory_usage_mb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        // Read from /proc/self/statm
        if let Ok(content) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(rss_pages) = content.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages.parse::<u64>() {
                    // Page size is typically 4096 bytes
                    return (pages * 4096) / (1024 * 1024);
                }
            }
        }
        0
    }

    #[cfg(target_os = "windows")]
    {
        use std::mem;
        use windows::Win32::System::ProcessStatus::*;
        use windows::Win32::System::Threading::*;

        unsafe {
            let handle = GetCurrentProcess();
            let mut pmc: PROCESS_MEMORY_COUNTERS = mem::zeroed();
            pmc.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

            if GetProcessMemoryInfo(handle, &mut pmc, pmc.cb).is_ok() {
                return pmc.WorkingSetSize as u64 / (1024 * 1024);
            }
        }
        0
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_creation() {
        let status = Status::new(100);
        assert_eq!(status.current_count.load(Ordering::Relaxed), 0);
        assert_eq!(status.total_count.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_status_increment() {
        let status = Status::new(100);
        status.increment();
        status.increment();
        assert_eq!(status.current_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_status_print_no_panic() {
        let status = Status::new(10);
        status.print_status(); // Should not panic with zero elapsed time
    }

    #[test]
    fn test_status_zero_total() {
        let status = Status::new(0);
        status.print_status(); // Should handle division by zero
    }

    #[tokio::test]
    async fn test_status_timer() {
        let status = Arc::new(Status::new(10));
        let handle = status.start_timer();
        // Let it run briefly then cancel
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.abort();
    }
}
