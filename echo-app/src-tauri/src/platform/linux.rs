use std::process::Command;

use crate::core::injection::TextInjector;
use crate::error::{EchoError, Result};

pub struct LinuxInjector;

impl LinuxInjector {
    pub fn new() -> Self {
        Self
    }
}

impl TextInjector for LinuxInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        // Pick the tool based on the active display server. ydotool talks to the
        // kernel uinput device (Wayland-friendly) but needs the ydotoold daemon
        // running; xdotool drives X11. Arguments are passed directly (never via a
        // shell) and the literal text follows `--` so it is never parsed as flags.
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let (program, args): (&str, Vec<&str>) = if wayland {
            ("ydotool", vec!["type", "--", text])
        } else {
            ("xdotool", vec!["type", "--clearmodifiers", "--", text])
        };

        let status = Command::new(program).args(&args).status().map_err(|e| {
            EchoError::Injection(format!(
                "Failed to run {program}: {e}. Is it installed? \
                 (Wayland needs ydotool + ydotoold; X11 needs xdotool.)"
            ))
        })?;

        if !status.success() {
            return Err(EchoError::Injection(format!(
                "{program} exited unsuccessfully ({status})"
            )));
        }
        Ok(())
    }

    fn send_paste(&self) -> Result<()> {
        // ydotool addresses keys by Linux input-event codes (29 = LEFTCTRL,
        // 47 = V); xdotool uses key names. Both send Ctrl+V.
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let (program, args): (&str, Vec<&str>) = if wayland {
            ("ydotool", vec!["key", "29:1", "47:1", "47:0", "29:0"])
        } else {
            ("xdotool", vec!["key", "--clearmodifiers", "ctrl+v"])
        };

        let status = Command::new(program).args(&args).status().map_err(|e| {
            EchoError::Injection(format!(
                "Failed to run {program}: {e}. Is it installed? \
                 (Wayland needs ydotool + ydotoold; X11 needs xdotool.)"
            ))
        })?;
        if !status.success() {
            return Err(EchoError::Injection(format!(
                "{program} exited unsuccessfully ({status})"
            )));
        }
        Ok(())
    }
}
