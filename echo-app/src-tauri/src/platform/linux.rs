use std::process::Command;

use crate::core::injection::{linux_chord_command, linux_type_command, TextInjector};
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

        // Which tool and arguments to use lives in `core::injection` so it can
        // be tested from any host — this module only compiles on Linux.
        // Arguments are passed directly, never through a shell.
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let (program, args) = linux_type_command(wayland, text);

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
    let (program, args) = linux_chord_command(wayland, keycode, xdotool_key);

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
