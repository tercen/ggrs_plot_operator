//! Memory and timing profiling utilities
//!
//! - Memory: Reads RSS (Resident Set Size) from /proc/self/status
//! - Timing: Uses std::time::Instant for precise measurements

use std::fs;
use std::time::Instant;

/// Get current RSS (Resident Set Size) in bytes from /proc/self/status
pub fn get_rss_bytes() -> u64 {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                // Format: "VmRSS:    123456 kB"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return kb * 1024; // Convert KB to bytes
                    }
                }
            }
        }
    }
    0
}

/// Get current RSS in megabytes
pub fn get_rss_mb() -> f64 {
    get_rss_bytes() as f64 / (1024.0 * 1024.0)
}

/// Print memory checkpoint with label
pub fn checkpoint(label: &str) {
    let rss = get_rss_mb();
    eprintln!("MEMPROF [{:.2} MB]: {}", rss, label);
}

/// Memory checkpoint that returns the value for comparison
pub fn checkpoint_return(label: &str) -> f64 {
    let rss = get_rss_mb();
    eprintln!("MEMPROF [{:.2} MB]: {}", rss, label);
    rss
}

/// Calculate delta from a previous checkpoint
pub fn delta(label: &str, previous: f64) -> f64 {
    let current = get_rss_mb();
    let delta = current - previous;
    eprintln!(
        "MEMPROF [{:.2} MB] (delta: {:+.2} MB): {}",
        current, delta, label
    );
    current
}

// ============================================================================
// Timing utilities
// ============================================================================

/// Start a new timing checkpoint, returns the Instant
pub fn time_start(label: &str) -> Instant {
    eprintln!("TIMEPROF: {} ...", label);
    Instant::now()
}

/// Print elapsed time from a previous checkpoint, returns new Instant for chaining
pub fn time_elapsed(label: &str, start: Instant) -> Instant {
    let elapsed = start.elapsed();
    eprintln!("TIMEPROF: {} [{:.3}s]", label, elapsed.as_secs_f64());
    Instant::now()
}

/// Print elapsed time with delta from previous checkpoint
pub fn time_delta(label: &str, start: Instant, section_start: Instant) -> Instant {
    let total = start.elapsed();
    let section = section_start.elapsed();
    eprintln!(
        "TIMEPROF: {} [+{:.3}s, total: {:.3}s]",
        label,
        section.as_secs_f64(),
        total.as_secs_f64()
    );
    Instant::now()
}
