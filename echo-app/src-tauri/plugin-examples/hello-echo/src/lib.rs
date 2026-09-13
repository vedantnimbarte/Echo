//! hello-echo — an Echo plugin.
//!
//! Echo calls `on_load` once when the plugin is enabled and `on_unload` when it
//! is disabled or Echo quits. In between, because this plugin returns itself
//! from `as_output`, Echo tells it about every transcript it delivers.
//!
//! What it does with them is deliberately dull: it notes how long each one was
//! and which app it went to, in a log in its data directory. The words
//! themselves are not written down — an example you install to see what a
//! plugin can do should not quietly start keeping a copy of your dictation.
//! `transcript.text` is right there when yours has a reason to.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use echo_sdk::{
    export_plugin, OutputPlugin, Plugin, PluginContext, PluginError, PluginResult, Transcript,
};

#[derive(Default)]
struct HelloEcho {
    /// Where the log lives, learned in `on_load`. Every hook takes `&self`
    /// because Echo calls them from more than one thread, so anything a plugin
    /// learns after it is created goes in a cell like this one.
    log: OnceLock<PathBuf>,
}

impl HelloEcho {
    fn append(&self, line: &str) -> PluginResult<()> {
        let path = self.log.get().ok_or("not loaded yet")?;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| PluginError::new(e.to_string()))?;
        writeln!(log, "{line}").map_err(|e| PluginError::new(e.to_string()))
    }
}

impl Plugin for HelloEcho {
    fn name(&self) -> &str {
        "hello-echo"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        // `ctx.data_dir` is the directory Echo hands you to keep files in.
        // Everything here returns a PluginError rather than panicking. Echo
        // survives a panic — `export_plugin!` catches it before it leaves the
        // library — but an error can say what went wrong.
        std::fs::create_dir_all(&ctx.data_dir).map_err(|e| PluginError::new(e.to_string()))?;
        let _ = self.log.set(ctx.data_dir.join("hello-echo.log"));
        self.append("hello-echo loaded")
    }

    fn on_unload(&self) -> PluginResult<()> {
        Ok(())
    }

    // The line that makes this an output plugin. Leave it out and Echo never
    // calls `on_transcript`, however carefully it is written.
    fn as_output(&self) -> Option<&dyn OutputPlugin> {
        Some(self)
    }
}

impl OutputPlugin for HelloEcho {
    // Called after the text has been typed, on a thread of its own, so nothing
    // in here can slow dictation down or lose what was said.
    fn on_transcript(&self, transcript: &Transcript) -> PluginResult<()> {
        let app = transcript.app.as_deref().unwrap_or("an unknown app");
        let chars = transcript.text.chars().count();
        self.append(&format!("delivered {chars} characters to {app}"))
    }
}

// Emits `echo_plugin_create` and `echo_plugin_abi_version`, the symbols Echo
// looks up after opening the library. Write them by hand and you own the
// unsafe, and the panic handling; this macro does both.
export_plugin!(HelloEcho);
