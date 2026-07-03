//! # echo-sdk
//!
//! The public API for writing **Echo** plugins. Echo loads plugins as native
//! shared libraries (`.dll` / `.dylib` / `.so`) at runtime, so a plugin and the
//! host must agree on the exact shape of the [`Plugin`] trait — that shared
//! definition lives here.
//!
//! A minimal plugin:
//!
//! ```
//! use echo_sdk::{export_plugin, Plugin, PluginContext, PluginResult};
//!
//! #[derive(Default)]
//! struct HelloPlugin;
//!
//! impl Plugin for HelloPlugin {
//!     fn name(&self) -> &str { "hello" }
//!     fn version(&self) -> &str { "0.1.0" }
//!     fn on_load(&self, _ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
//!     fn on_unload(&self) -> PluginResult<()> { Ok(()) }
//! }
//!
//! // Emits the `echo_plugin_create` entry point the host looks for.
//! export_plugin!(HelloPlugin);
//! ```
//!
//! Ship the compiled library next to a `plugin.json` describing it (see
//! [`PluginManifest`]).

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Error returned by plugin lifecycle hooks.
///
/// Deliberately dependency-free (just a message) so the SDK stays decoupled
/// from the host's error type and async runtime. The host converts this into
/// its own error on the boundary.
#[derive(Debug, Clone)]
pub struct PluginError(pub String);

impl PluginError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PluginError {}

impl From<String> for PluginError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for PluginError {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Result alias used throughout the plugin API.
pub type PluginResult<T> = std::result::Result<T, PluginError>;

/// Read-only accessor a plugin uses to query host settings. Returns `None` for
/// keys the host does not expose to plugins.
pub type SettingsAccessor = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Declared capabilities a plugin may request in its manifest. The permission
/// model is advisory in this version (plugins run in-process); true sandboxing
/// (e.g. a WASM runtime) is a future goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginPermission {
    Asr,
    Output,
    Audio,
    Dictionary,
}

/// Manifest shipped alongside a plugin as `plugin.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    /// Shared-library file name (e.g. `my_plugin.dll`).
    pub entry: String,
}

/// Summary of an installed plugin returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
}

/// Runtime context handed to a plugin on load.
pub struct PluginContext {
    /// Directory the plugin may use for its own data files.
    pub data_dir: PathBuf,
    /// Read-only accessor for host settings.
    pub settings: SettingsAccessor,
}

/// Base trait every plugin implements. `Send + Sync` because plugins live in
/// shared application state.
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn on_load(&self, ctx: &PluginContext) -> PluginResult<()>;
    fn on_unload(&self) -> PluginResult<()>;
}

/// A plugin that injects text into the system.
pub trait OutputPlugin: Plugin {
    fn inject_text(&self, text: &str) -> PluginResult<()>;
}

/// A plugin that processes captured audio in place.
pub trait AudioPlugin: Plugin {
    fn process(&self, samples: &mut Vec<f32>);
}

/// Emit the `echo_plugin_create` FFI entry point the host loader looks up.
///
/// The plugin type must implement [`Plugin`] and [`Default`]. The host takes
/// ownership of the returned boxed trait object.
///
/// ```
/// # use echo_sdk::{export_plugin, Plugin, PluginContext, PluginResult};
/// #[derive(Default)]
/// struct MyPlugin;
/// impl Plugin for MyPlugin {
///     fn name(&self) -> &str { "my-plugin" }
///     fn version(&self) -> &str { "0.1.0" }
///     fn on_load(&self, _: &PluginContext) -> PluginResult<()> { Ok(()) }
///     fn on_unload(&self) -> PluginResult<()> { Ok(()) }
/// }
/// export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin_ty:ty) => {
        /// FFI constructor called by the Echo host after `dlopen`.
        #[no_mangle]
        pub extern "C" fn echo_plugin_create() -> *mut ::std::boxed::Box<dyn $crate::Plugin> {
            let plugin: ::std::boxed::Box<dyn $crate::Plugin> =
                ::std::boxed::Box::new(<$plugin_ty as ::std::default::Default>::default());
            ::std::boxed::Box::into_raw(::std::boxed::Box::new(plugin))
        }
    };
}
