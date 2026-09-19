use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::{
    core::lock::LockLive,
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
#[specta::specta]
pub fn list_plugins(state: State<'_, AppState>) -> Result<Vec<PluginInfo>> {
    let conn = state.db.lock().unwrap();
    let rows = repositories::list_plugins(&conn)?;
    let infos = rows
        .into_iter()
        .map(|row| {
            let manifest = serde_json::from_str::<PluginManifest>(&row.manifest).ok();
            PluginInfo {
                name: row.name,
                version: manifest
                    .as_ref()
                    .map(|m| m.version.clone())
                    .unwrap_or_default(),
                description: manifest
                    .as_ref()
                    .map(|m| m.description.clone())
                    .unwrap_or_default(),
                author: manifest
                    .as_ref()
                    .map(|m| m.author.clone())
                    .unwrap_or_default(),
                enabled: row.enabled,
                permissions: manifest.map(|m| m.permissions).unwrap_or_default(),
            }
        })
        .collect();
    Ok(infos)
}

/// Read the manifest next to a candidate library **without installing it**, so
/// the UI can show what the plugin claims it needs before anything is loaded.
#[tauri::command]
#[specta::specta]
pub fn inspect_plugin(path: String) -> Result<PluginManifest> {
    let lib_path = PathBuf::from(&path);
    let src_dir = lib_path
        .parent()
        .ok_or_else(|| EchoError::Plugin("Invalid plugin path".into()))?;
    let manifest_str = std::fs::read_to_string(src_dir.join("plugin.json"))
        .map_err(|e| EchoError::Plugin(format!("plugin.json not found next to library: {e}")))?;
    Ok(serde_json::from_str(&manifest_str)?)
}

/// Install a plugin from a shared library path. Expects a `plugin.json` manifest
/// in the same directory. Copies both into the plugins directory, registers it,
/// and loads it.
///
/// `acknowledged` must be true: the caller has to have shown the user the
/// declared permissions *and* the fact that a plugin runs in-process with full
/// host privileges. This is a consent gate, not a sandbox — it cannot stop a
/// malicious plugin, it only stops one being loaded without the user being told.
#[tauri::command]
#[specta::specta]
pub async fn install_plugin(
    state: State<'_, AppState>,
    path: String,
    acknowledged: bool,
) -> Result<()> {
    if !acknowledged {
        return Err(EchoError::PermissionDenied(
            "Plugin install was not confirmed. Plugins run in-process with full              access to Echo and your files; the permission list in the manifest              is advisory and is not enforced."
                .into(),
        ));
    }

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

    // Fingerprint the copy, not the source: the copy is what will be loaded,
    // and hashing the original would certify a file Echo never runs again.
    let installed_lib = dest_dir.join(&manifest.entry);
    let fingerprint = crate::core::plugins::integrity::fingerprint(&installed_lib)?;

    {
        let conn = state.db.lock().unwrap();
        repositories::upsert_plugin(
            &conn,
            &manifest.name,
            &manifest.version,
            true,
            &manifest_str,
            Some(&fingerprint),
        )?;
    }

    let ctx = make_context(state.plugins_dir.clone());
    let lib = installed_lib_path(&state.plugins_dir, &manifest);
    state.plugins.lock_live().load(&lib, &ctx)?;
    sync_capabilities(&state).await;
    Ok(())
}

/// Bring every capability that is registered rather than dispatched per call
/// back in line with what is loaded: dictionary entries and ASR engines.
///
/// Output and audio need nothing here — they read the loaded set when a
/// recording starts. These two are held elsewhere (the dictionary engine, the
/// ASR manager) and would otherwise keep a disabled plugin's contribution, or
/// never see a newly enabled one. Called after every load or unload, and once
/// at startup.
pub(crate) async fn sync_capabilities(state: &AppState) {
    use crate::core::asr::AsrProvider;
    use crate::core::plugins::dispatch::{PluginAsrProvider, ASR_PREFIX};

    let plugins = state.plugins.lock_live().plugins();

    state.asr.unregister_prefixed(ASR_PREFIX).await;
    for provider in plugins.into_iter().filter_map(PluginAsrProvider::new) {
        tracing::info!(provider = provider.name(), "Plugin engine available");
        state.asr.register(Arc::new(provider)).await;
    }

    let raw = {
        let conn = state.db.lock_live();
        repositories::list_dictionary_entries(&conn)
    };
    match raw {
        Ok(raw) => crate::commands::dictionary::refresh_engine(state, raw).await,
        Err(e) => tracing::error!("Couldn't rebuild the dictionary after a plugin change: {e}"),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn enable_plugin(state: State<'_, AppState>, name: String) -> Result<()> {
    let manifest = read_installed_manifest(&state.plugins_dir, &name)?;
    {
        let conn = state.db.lock().unwrap();
        repositories::set_plugin_enabled(&conn, &name, true)?;
    }
    let ctx = make_context(state.plugins_dir.clone());
    let lib = installed_lib_path(&state.plugins_dir, &manifest);

    // Same check the startup path makes: enabling is a load, and a library that
    // changed since install is not the one that was agreed to.
    {
        let conn = state.db.lock().unwrap();
        let recorded = repositories::list_plugins(&conn)?
            .into_iter()
            .find(|p| p.name == name)
            .and_then(|p| p.lib_sha256);
        let verdict = crate::core::plugins::integrity::verify(&lib, recorded.as_deref())?;
        if !verdict.is_trusted() {
            repositories::set_plugin_enabled(&conn, &name, false)?;
            return Err(EchoError::PermissionDenied(verdict.refusal(&name)));
        }
        if let crate::core::plugins::integrity::Verdict::FirstSeen(hash) = verdict {
            repositories::set_plugin_fingerprint(&conn, &name, &hash)?;
        }
    }

    let loaded = {
        let mut loader = state.plugins.lock_live();
        if loader.is_loaded(&name) {
            Ok(())
        } else {
            loader.load(&lib, &ctx).map(|_| ())
        }
    };
    if let Err(e) = loaded {
        // Refused — most often a library built against an older echo-sdk. Left
        // marked enabled, the toggle would claim a plugin that is not running.
        let conn = state.db.lock_live();
        repositories::set_plugin_enabled(&conn, &name, false)?;
        return Err(e);
    }
    sync_capabilities(&state).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn disable_plugin(state: State<'_, AppState>, name: String) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_plugin_enabled(&conn, &name, false)?;
    }
    state.plugins.lock_live().unload(&name)?;
    sync_capabilities(&state).await;
    Ok(())
}

/// Write a starter plugin project into `parent_dir/<name>/` and return the
/// directory it created.
///
/// `parent_dir` comes from the native folder picker, so the user has already
/// chosen where this lands. `name` is typed, which is why it is validated
/// rather than trusted: it becomes a path segment underneath that folder.
#[tauri::command]
#[specta::specta]
pub fn scaffold_plugin(parent_dir: String, name: String) -> Result<String> {
    let dir = crate::core::plugins::scaffold::write(&PathBuf::from(parent_dir), &name)?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_plugin(state: State<'_, AppState>, name: String) -> Result<()> {
    state.plugins.lock_live().unload(&name)?;
    // Before the directory goes: a registered plugin engine holds the library
    // open, and Windows will not delete a DLL that is still mapped.
    sync_capabilities(&state).await;
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
