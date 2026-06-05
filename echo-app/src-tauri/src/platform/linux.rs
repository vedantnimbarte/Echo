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
        // Phase 3: implement via xdotool (X11) or ydotool (Wayland).
        Err(EchoError::Injection("Linux injection not yet implemented".into()))
    }
}
