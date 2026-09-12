//! How much memory this process is actually using, and how long it took to
//! start.
//!
//! Both had targets that were never measured, so the published numbers were
//! aspirations. This is what `--benchmark` reads to replace them with facts.
//!
//! **Resident set, not virtual size.** Virtual size counts address space the
//! process reserved and may never touch — a whisper model mapped but not read
//! shows up there and misleads. Resident is what the machine is actually
//! giving us, which is what "uses 400 MB" means to a person.
//!
//! ponytail: three implementations because there is no portable way to ask,
//! and macOS gets the least elegant one — `ps` rather than a `mach` FFI, since
//! this is a diagnostic that runs a handful of times per benchmark and a whole
//! new dependency for it would cost more than it saves. Windows uses the crate
//! already in the tree; Linux reads a file.

use std::sync::OnceLock;
use std::time::Instant;

/// When the process began, recorded as early as `main` can manage.
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Record the process start. Called first thing in `main`; calling it again
/// does nothing, so the earliest caller wins.
pub fn mark_start() {
    let _ = STARTED.set(Instant::now());
}

/// How long ago [`mark_start`] ran, or `None` if it never did.
///
/// This measures process start to whenever it is asked — for the benchmark,
/// that is the end of Tauri's `setup`. It is not "time to first pixel", which
/// Rust cannot see; it is everything Echo itself controls, which is the part a
/// change to Echo could make faster.
pub fn since_start() -> Option<std::time::Duration> {
    STARTED.get().map(Instant::elapsed)
}

/// Resident memory of this process, in bytes.
///
/// `None` when the platform declines to say, which is reported as unknown
/// rather than guessed at.
pub fn resident_bytes() -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;

        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        unsafe {
            GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb)
                .ok()
                .map(|()| counters.WorkingSetSize as u64)
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Field 2 of statm is resident pages. Multiplying by the page size is
        // the whole conversion.
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(pages * 4096)
    }

    #[cfg(target_os = "macos")]
    {
        // `ps` reports RSS in kilobytes. A subprocess to read a number is
        // clumsy, and it is still less code than a mach FFI plus the crate it
        // would need — see the module note.
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(kb * 1024)
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    None
}

/// Bytes as megabytes, for a report a person reads.
pub fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every platform the app ships on can answer, and the answer is a
    /// plausible size for a running process rather than zero.
    #[test]
    fn this_platform_reports_its_own_memory() {
        let bytes = resident_bytes().expect("a supported platform must answer");
        assert!(
            bytes > 1024 * 1024,
            "a running test process uses more than a megabyte, got {bytes}"
        );
    }

    #[test]
    fn megabytes_render_without_noise() {
        assert_eq!(mb(1024 * 1024), "1 MB");
        assert_eq!(mb(100 * 1024 * 1024), "100 MB");
    }

    /// Start time is only available once `main` has marked it, and `main` does
    /// not run under the test harness — so absence is the correct answer here,
    /// and the getter must not panic reaching for it.
    #[test]
    fn an_unmarked_start_is_absent_rather_than_wrong() {
        assert!(since_start().is_none());
    }
}
