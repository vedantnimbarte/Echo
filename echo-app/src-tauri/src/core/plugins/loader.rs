use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use libloading::{Library, Symbol};

use super::{Plugin, PluginContext};
use crate::error::{EchoError, Result};

/// Symbol a plugin shared library must export. It returns a heap-allocated boxed
/// trait object that the host takes ownership of. Plugin authors generate this
/// with `echo_sdk::export_plugin!(MyPlugin)` rather than writing it by hand.
type CreateFn = unsafe extern "C" fn() -> *mut Box<dyn Plugin>;

/// Symbol reporting which `echo_sdk::ABI_VERSION` the library was built
/// against. Plain C ABI, so it can be read safely from any library at all —
/// which is the point: it is the check that decides whether calling
/// `CreateFn` is safe.
type AbiVersionFn = unsafe extern "C" fn() -> u32;

/// One plugin Echo has loaded, and the library its code lives in.
///
/// Shared as an `Arc` so a capability call can run without holding the
/// loader's lock — an output plugin posting to a webhook must not freeze the
/// Plugins page — and so the library cannot be unmapped underneath a call
/// that is still running: the last clone to drop is what closes it.
pub struct LoadedPlugin {
    // `plugin` is declared before `_lib` so it is dropped first — its vtable
    // lives inside the library, which must outlive it.
    plugin: Box<dyn Plugin>,
    /// `None` only for plugins compiled into this crate by tests.
    _lib: Option<Library>,
    /// Set when the audio hook failed; it is not called again for this load.
    /// See `dispatch::process_audio`.
    pub(crate) audio_benched: AtomicBool,
}

impl LoadedPlugin {
    fn new(plugin: Box<dyn Plugin>, lib: Option<Library>) -> Self {
        Self {
            plugin,
            _lib: lib,
            audio_benched: AtomicBool::new(false),
        }
    }

    pub fn plugin(&self) -> &dyn Plugin {
        self.plugin.as_ref()
    }
}

/// Loads and unloads native plugin libraries.
///
/// Plugins run in-process with full trust: loading arbitrary native code is
/// inherently unsafe, and the manifest permission list is advisory only. The
/// user must explicitly install and enable each plugin.
///
/// **Loaded means enabled.** Enabling a plugin loads it, disabling unloads it,
/// and startup loads only rows marked enabled, so the dispatchers need no
/// enabled flag of their own: whatever [`Self::plugins`] returns is exactly the
/// set allowed to take part.
#[derive(Default)]
pub struct PluginLoader {
    loaded: Vec<Arc<LoadedPlugin>>,
}

impl PluginLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a shared library, instantiate its plugin, and call `on_load`.
    /// Returns the plugin's reported name.
    pub fn load(&mut self, lib_path: &Path, ctx: &PluginContext) -> Result<String> {
        tracing::warn!(
            "Loading native plugin {} — plugins run IN-PROCESS with the same \
             privileges as Echo itself. Manifest permissions are advisory and \
             are NOT enforced.",
            lib_path.display()
        );

        // SAFETY: dlopen-ing arbitrary code is unsafe by nature; this is gated
        // behind explicit user install/enable.
        let lib = unsafe { Library::new(lib_path) }.map_err(|e| {
            EchoError::Plugin(format!("Failed to load {}: {e}", lib_path.display()))
        })?;

        // Before anything that touches the trait object. A library without the
        // marker was built against echo-sdk 0.1, whose vtable is shorter than
        // the one this host indexes into.
        let abi = unsafe {
            lib.get::<AbiVersionFn>(b"echo_plugin_abi_version\0")
                .ok()
                .map(|f| f())
        };
        check_abi(abi, lib_path)?;

        let plugin: Box<dyn Plugin> = unsafe {
            let create: Symbol<CreateFn> = lib
                .get(b"echo_plugin_create\0")
                .map_err(|e| EchoError::Plugin(format!("Missing echo_plugin_create: {e}")))?;
            let raw = create();
            if raw.is_null() {
                return Err(EchoError::Plugin("echo_plugin_create returned null".into()));
            }
            *Box::from_raw(raw)
        };

        plugin.on_load(ctx)?;
        let name = plugin.name().to_string();
        self.loaded
            .push(Arc::new(LoadedPlugin::new(plugin, Some(lib))));
        Ok(name)
    }

    /// Call `on_unload` and drop the plugin. Its library closes when the last
    /// in-flight capability call holding it returns.
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

    /// Every loaded — which is to say enabled — plugin, for a dispatcher to
    /// call without holding this loader's lock.
    pub fn plugins(&self) -> Vec<Arc<LoadedPlugin>> {
        self.loaded.clone()
    }

    /// Add a plugin compiled into this crate, as a test's stand-in for a
    /// library. Calls `on_load` like the real path does.
    #[cfg(test)]
    pub fn load_in_process(&mut self, plugin: Box<dyn Plugin>, ctx: &PluginContext) -> Result<()> {
        plugin.on_load(ctx)?;
        self.loaded.push(Arc::new(LoadedPlugin::new(plugin, None)));
        Ok(())
    }
}

/// Refuse a library built against a different SDK shape.
///
/// Refused, not warned about: a mismatch is not a plugin that might misbehave,
/// it is one whose every capability call reads the wrong vtable slot. The only
/// fix is a rebuild, so the message says exactly that.
fn check_abi(found: Option<u32>, lib_path: &Path) -> Result<()> {
    let expected = echo_sdk::ABI_VERSION;
    match found {
        Some(v) if v == expected => Ok(()),
        Some(v) => Err(EchoError::Plugin(format!(
            "{} was built for plugin ABI {v}, and this Echo speaks ABI {expected}. \
             Rebuild it against the echo-sdk this Echo ships with.",
            lib_path.display()
        ))),
        None => Err(EchoError::Plugin(format!(
            "{} was built against echo-sdk 0.1, which this Echo can no longer load \
             safely. Rebuild it against echo-sdk 0.2 — the code needs no changes \
             unless it wants the new capabilities.",
            lib_path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_matching_abi_is_loaded() {
        let path = Path::new("thing.dll");
        assert!(check_abi(Some(echo_sdk::ABI_VERSION), path).is_ok());

        let old = check_abi(None, path).unwrap_err().to_string();
        assert!(old.contains("Rebuild"), "{old}");

        let other = check_abi(Some(echo_sdk::ABI_VERSION + 1), path)
            .unwrap_err()
            .to_string();
        assert!(other.contains("Rebuild"), "{other}");
    }
}
