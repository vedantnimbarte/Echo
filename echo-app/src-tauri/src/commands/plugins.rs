use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::{
    core::plugins::{PluginContext, PluginInfo, PluginManifest},
    error::{EchoError, Result},
    state::AppState,
    storage::repositories,
};

fn make_context(data_dir: PathBuf) -> PluginContext {
    // Settings access from plugins is limited in this advisory version.
    PluginContext {
        data_dir,
        settings: Arc::new(|_key| None),
    }
}

/// Read the manifest stored in a plugin's install directory.
fn read_installed_manifest(plugins_dir: &PathBuf, name: &str) -> Result<PluginManifest> {
    let manifest_path = plugins_dir.join(name).join("plugin.json");
    let s = std::fs::read_to_string(&manifest_path)
        .map_err(|e| EchoError::Plugin(format!("Missing manifest for '{name}': {e}")))?;
    Ok(serde_json::from_str(&s)?)
}

fn installed_lib_path(plugins_dir: &PathBuf, manifest: &PluginManifest) -> PathBuf {
    plugins_dir.join(&manifest.name).join(&manifest.entry)
}

#[tauri::command]
pub fn list_plugins(state: State<'_, AppState>) -> Result<Vec<PluginInfo>> {
    let conn = state.db.lock().unwrap();
    let rows = repositories::list_plugins(&conn)?;
    let infos = rows
        .into_iter()
        .map(|(name, version, enabled, manifest)| {
            let (description, author) = serde_json::from_str::<PluginManifest>(&manifest)
                .map(|m| (m.description, m.author))
                .unwrap_or_default();
            PluginInfo {
                name,
                version,
                description,
                author,
                enabled,
            }
        })
        .collect();
    Ok(infos)
}

/// Install a plugin from a shared library path. Expects a `plugin.json` manifest
/// in the same directory. Copies both into the plugins directory, registers it,
/// and loads it.
#[tauri::command]
pub fn install_plugin(state: State<'_, AppState>, path: String) -> Result<()> {
    let lib_path = PathBuf::from(&path);
    let src_dir = lib_path
        .parent()
        .ok_or_else(|| EchoError::Plugin("Invalid plugin path".into()))?;
    let manifest_str = std::fs::read_to_string(src_dir.join("plugin.json"))
        .map_err(|e| EchoError::Plugin(format!("plugin.json not found next to library: {e}")))?;
    let manifest: PluginManifest = serde_json::from_str(&manifest_str)?;

    let dest_dir = state.plugins_dir.join(&manifest.name);
    std::fs::create_dir_all(&dest_dir).map_err(|e| EchoError::Plugin(e.to_string()))?;
    std::fs::copy(&lib_path, dest_dir.join(&manifest.entry))
        .map_err(|e| EchoError::Plugin(e.to_string()))?;
    std::fs::write(dest_dir.join("plugin.json"), &manifest_str)
        .map_err(|e| EchoError::Plugin(e.to_string()))?;

    {
        let conn = state.db.lock().unwrap();
        repositories::upsert_plugin(&conn, &manifest.name, &manifest.version, true, &manifest_str)?;
    }

    let ctx = make_context(state.plugins_dir.clone());
    let lib = installed_lib_path(&state.plugins_dir, &manifest);
    state.plugins.lock().unwrap().load(&lib, &ctx)?;
    Ok(())
}

#[tauri::command]
pub fn enable_plugin(state: State<'_, AppState>, name: String) -> Result<()> {
    let manifest = read_installed_manifest(&state.plugins_dir, &name)?;
    {
        let conn = state.db.lock().unwrap();
        repositories::set_plugin_enabled(&conn, &name, true)?;
    }
    let ctx = make_context(state.plugins_dir.clone());
    let lib = installed_lib_path(&state.plugins_dir, &manifest);
    let mut loader = state.plugins.lock().unwrap();
    if !loader.is_loaded(&name) {
        loader.load(&lib, &ctx)?;
    }
    Ok(())
}

#[tauri::command]
pub fn disable_plugin(state: State<'_, AppState>, name: String) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_plugin_enabled(&conn, &name, false)?;
    }
    state.plugins.lock().unwrap().unload(&name)?;
    Ok(())
}

#[tauri::command]
pub fn uninstall_plugin(state: State<'_, AppState>, name: String) -> Result<()> {
    state.plugins.lock().unwrap().unload(&name)?;
    {
        let conn = state.db.lock().unwrap();
        repositories::delete_plugin(&conn, &name)?;
    }
    let dir = state.plugins_dir.join(&name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| EchoError::Plugin(e.to_string()))?;
    }
    Ok(())
}
