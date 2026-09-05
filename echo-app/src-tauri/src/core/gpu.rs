//! GPU capability detection for the local whisper backends.
//!
//! Echo prefers the GPU whenever one is usable and falls back to the CPU
//! otherwise. "Usable" is deliberately narrow: it means we can *name* a
//! whisper.cpp build that will run on this machine, not merely that a graphics
//! card exists. A card we have no matching binary for is the same as no card.
//!
//! Detection is a process spawn, so it runs once at startup and the result is
//! carried around rather than re-derived.

use std::process::Command;

/// Which accelerated whisper.cpp build this machine can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    /// NVIDIA CUDA. `major` is the CUDA generation the *driver* supports, which
    /// is what decides which cuBLAS pack we may install — a driver reports the
    /// highest toolkit it can run, and older toolkits stay compatible.
    Cuda { major: u32 },
    /// Apple Metal. Not a separate download: every macOS whisper.cpp build has
    /// it compiled in, so there is nothing to install and nothing to select.
    Metal,
    /// CPU only.
    None,
}

impl GpuBackend {
    /// Short label for the settings UI and logs.
    pub fn label(self) -> String {
        match self {
            GpuBackend::Cuda { major } => format!("NVIDIA CUDA {major}.x"),
            GpuBackend::Metal => "Apple Metal".into(),
            GpuBackend::None => "CPU only".into(),
        }
    }
}

/// Detect the best available backend for this machine.
pub fn detect() -> GpuBackend {
    // macOS ships Metal in the stock build; there is no pack to choose and no
    // probe worth paying for.
    if cfg!(target_os = "macos") {
        return GpuBackend::Metal;
    }

    match nvidia_cuda_major() {
        Some(major) => GpuBackend::Cuda { major },
        None => GpuBackend::None,
    }
}

/// Ask the NVIDIA driver which CUDA generation it supports.
///
/// `nvidia-smi` is installed alongside every NVIDIA driver, so its absence is a
/// reliable "no usable NVIDIA GPU here" and not something to warn about.
fn nvidia_cuda_major() -> Option<u32> {
    let output = new_command("nvidia-smi").output().ok()?;
    if !output.status.success() {
        return None;
    }
    // The header line reads: "... CUDA Version: 12.4 ...".
    parse_cuda_major(&String::from_utf8_lossy(&output.stdout))
}

/// Pull the CUDA major version out of `nvidia-smi` header text.
fn parse_cuda_major(text: &str) -> Option<u32> {
    let tail = text.split("CUDA Version:").nth(1)?;
    let version = tail.split_whitespace().next()?;
    version.split('.').next()?.parse().ok()
}

/// A `Command` that never flashes a console window on Windows.
///
/// Without this, every probe pops a black console for a few frames on top of
/// whatever the user is working in.
fn new_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cuda_version_out_of_smi_output() {
        let header = "\
| NVIDIA-SMI 551.86    Driver Version: 551.86    CUDA Version: 12.4     |
|-----------------------------------------+----------------------+";
        assert_eq!(parse_cuda_major(header), Some(12));
    }

    #[test]
    fn older_drivers_report_their_own_generation() {
        assert_eq!(parse_cuda_major("CUDA Version: 11.8 "), Some(11));
    }

    #[test]
    fn output_without_a_version_is_not_a_gpu() {
        // A driver too old to print the field, or a command that printed an
        // error — both mean "don't try to use CUDA", not "assume the newest".
        assert_eq!(parse_cuda_major("no such field here"), None);
        assert_eq!(parse_cuda_major("CUDA Version: unknown"), None);
        assert_eq!(parse_cuda_major(""), None);
    }

    #[test]
    fn backends_describe_themselves_for_the_ui() {
        assert_eq!(GpuBackend::None.label(), "CPU only");
        assert_eq!(GpuBackend::Cuda { major: 12 }.label(), "NVIDIA CUDA 12.x");
        assert_eq!(GpuBackend::Metal.label(), "Apple Metal");
    }
}
