//! hello-echo — an Echo plugin.
//!
//! Echo calls `on_load` once when the plugin is enabled and `on_unload` when it
//! is disabled or Echo quits. This one writes a line into its own data
//! directory so there is something to look at after the first install.

use std::io::Write;

use echo_sdk::{export_plugin, Plugin, PluginContext, PluginError, PluginResult};

#[derive(Default)]
struct HelloEcho;

impl Plugin for HelloEcho {
    fn name(&self) -> &str {
        "hello-echo"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        // `ctx.data_dir` is the directory Echo hands you to keep files in.
        // Everything here returns a PluginError rather than panicking: a panic
        // crosses the library boundary into Echo and takes the app with it.
        std::fs::create_dir_all(&ctx.data_dir).map_err(|e| PluginError::new(e.to_string()))?;

        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ctx.data_dir.join("hello-echo.log"))
            .map_err(|e| PluginError::new(e.to_string()))?;

        writeln!(log, "hello-echo loaded").map_err(|e| PluginError::new(e.to_string()))?;
        Ok(())
    }

    fn on_unload(&self) -> PluginResult<()> {
        Ok(())
    }
}

// Emits `echo_plugin_create`, the one symbol Echo looks up after opening the
// library. Write it by hand and you own the unsafe; this macro does not.
export_plugin!(HelloEcho);
