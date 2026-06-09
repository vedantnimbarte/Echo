use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::asr::AsrProvider;
use crate::core::dictionary::DictionaryEntry;
use crate::error::Result;

pub mod loader;

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
    pub data_dir: PathBuf,
    pub settings: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
}

/// Base trait every plugin implements. `Send + Sync` because plugins live in
/// shared application state.
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn on_load(&self, ctx: &PluginContext) -> Result<()>;
    fn on_unload(&self) -> Result<()>;
}

/// A plugin that contributes an ASR provider.
pub trait AsrPlugin: Plugin {
    fn as_asr_provider(&self) -> Arc<dyn AsrProvider>;
}

/// A plugin that injects text into the system.
pub trait OutputPlugin: Plugin {
    fn inject_text(&self, text: &str) -> Result<()>;
}

/// A plugin that processes captured audio in place.
pub trait AudioPlugin: Plugin {
    fn process(&self, samples: &mut Vec<f32>);
}

/// A plugin that contributes dictionary entries.
pub trait DictionaryPlugin: Plugin {
    fn entries(&self) -> Vec<DictionaryEntry>;
}
