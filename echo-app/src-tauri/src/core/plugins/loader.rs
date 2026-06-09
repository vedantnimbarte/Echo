use std::path::Path;

use libloading::{Library, Symbol};

use super::{Plugin, PluginContext};
use crate::error::{EchoError, Result};

/// Symbol a plugin shared library must export. It returns a heap-allocated boxed
/// trait object that the host takes ownership of.
///
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn echo_plugin_create() -> *mut Box<dyn echo_sdk::Plugin> {
///     Box::into_raw(Box::new(Box::new(MyPlugin::default())))
/// }
/// ```
type CreateFn = unsafe extern "C" fn() -> *mut Box<dyn Plugin>;

struct LoadedPlugin {
    // `plugin` is declared before `_lib` so it is dropped first — its vtable
    // lives inside the library, which must outlive it.
    plugin: Box<dyn Plugin>,
    _lib: Library,
}

/// Loads and unloads native plugin libraries.
///
/// Plugins run in-process with full trust: loading arbitrary native code is
/// inherently unsafe, and the manifest permission list is advisory only. The
/// user must explicitly install and enable each plugin.
#[derive(Default)]
pub struct PluginLoader {
    loaded: Vec<LoadedPlugin>,
}

impl PluginLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a shared library, instantiate its plugin, and call `on_load`.
    /// Returns the plugin's reported name.
    pub fn load(&mut self, lib_path: &Path, ctx: &PluginContext) -> Result<String> {
        // SAFETY: dlopen-ing arbitrary code is unsafe by nature; this is gated
        // behind explicit user install/enable.
        let lib = unsafe { Library::new(lib_path) }.map_err(|e| {
            EchoError::Plugin(format!("Failed to load {}: {e}", lib_path.display()))
        })?;

        let plugin: Box<dyn Plugin> = unsafe {
            let create: Symbol<CreateFn> = lib
                .get(b"echo_plugin_create\0")
                .map_err(|e| EchoError::Plugin(format!("Missing echo_plugin_create: {e}")))?;
            let raw = create();
            if raw.is_null() {
                return Err(EchoError::Plugin(
                    "echo_plugin_create returned null".into(),
                ));
            }
            *Box::from_raw(raw)
        };

        plugin.on_load(ctx)?;
        let name = plugin.name().to_string();
        self.loaded.push(LoadedPlugin { plugin, _lib: lib });
        Ok(name)
    }

    /// Call `on_unload` and drop the plugin and its library.
    pub fn unload(&mut self, name: &str) -> Result<()> {
        if let Some(pos) = self.loaded.iter().position(|p| p.plugin.name() == name) {
            let p = self.loaded.remove(pos);
            p.plugin.on_unload()?;
        }
        Ok(())
    }

    pub fn is_loaded(&self, name: &str) -> bool {
        self.loaded.iter().any(|p| p.plugin.name() == name)
    }
}
