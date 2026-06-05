use crate::core::injection::TextInjector;
use crate::error::{EchoError, Result};

pub struct MacosInjector;

impl MacosInjector {
    pub fn new() -> Self {
        Self
    }
}

impl TextInjector for MacosInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        // Phase 3: implement via CGEventCreateKeyboardEvent / Accessibility API.
        // Requires accessibility permission from macOS.
        Err(EchoError::Injection("macOS injection not yet implemented".into()))
    }
}
