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
        send_ctrl_chord(47, "ctrl+v")
    }

    fn send_copy(&self) -> Result<()> {
        send_ctrl_chord(46, "ctrl+c")
    }
}

/// Send a Ctrl chord. ydotool addresses keys by Linux input-event code
/// (29 = LEFTCTRL, 46 = C, 47 = V); xdotool uses the key name.
fn send_ctrl_chord(keycode: u8, xdotool_key: &str) -> Result<()> {
    let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let down = format!("{keycode}:1");
    let up = format!("{keycode}:0");
    let (program, args): (&str, Vec<&str>) = if wayland {
        ("ydotool", vec!["key", "29:1", &down, &up, "29:0"])
    } else {
        ("xdotool", vec!["key", "--clearmodifiers", xdotool_key])
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
