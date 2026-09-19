mod benchmark;
mod cli;
mod commands;
mod core;
mod error;
mod ipc;
mod platform;
mod registry;
mod selftest;
mod state;
mod storage;
mod tray;

#[cfg(test)]
mod layering;

#[cfg(test)]
mod pipeline_tests;

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

use core::{
    asr::binary_manager::BinaryManager, asr::manager::AsrManager, asr::model_manager::ModelManager,
    audio::AudioService, dictionary::DictionaryEngine, injection::platform_injector,
};
use state::AppState;
use storage::db;

/// Send panics to `echo.log` as well as to stderr.
///
/// A panic inside a background task is otherwise invisible in a packaged build:
/// the task dies, the user sees dictation stop working, and the only record goes
/// to a console nobody is watching. Echo has no crash reporter by design, so the
/// log file is the whole story — this is the difference between "Echo broke" and
/// a report somebody can act on.
///
/// Chained rather than replacing the default hook, so the usual message and any
/// `RUST_BACKTRACE` output still appear.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        default(info);
    }));
}

/// Log to stdout *and* to `echo.log` beside the database.
///
/// Everything downstream of capture — VAD, ASR, injection — reports failure by
/// logging and carrying on, which is invisible in a packaged build and in any
/// dev run whose console has scrolled away. The file is the only record a user
/// can actually send us.
fn init_tracing(data_dir: &std::path::Path) {
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    let filter = || EnvFilter::from_default_env().add_directive("echo=debug".parse().unwrap());

    // Best effort: if the log file can't be opened, stdout alone still works.
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_dir.join("echo.log"))
        .ok();

    match file {
        Some(file) => tracing_subscriber::fmt()
            .with_env_filter(filter())
            .with_ansi(false)
            .with_writer(std::io::stdout.and(std::sync::Mutex::new(file)))
            .init(),
        None => tracing_subscriber::fmt().with_env_filter(filter()).init(),
    }
}

/// Record the process start time. Called by `main` before anything else; see
/// [`core::procinfo`].
pub fn mark_start() {
    core::procinfo::mark_start();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = ipc_builder();

    // Regenerate the TypeScript bindings on every debug run. This is the
    // upstream pattern and it is here rather than in a test for a concrete
    // reason: a test binary gets no side-by-side manifest, so it loads
    // comctl32 v5 while Wry's Windows paths need v6, and the process dies at
    // load with STATUS_ENTRYPOINT_NOT_FOUND before any test runs.
    // `bindings_export::every_command_is_in_the_bindings` is the guard that
    // catches a command added without a dev run since.
    #[cfg(debug_assertions)]
    {
        let exporter = specta_typescript::Typescript::default()
            // i64 as `number`, not the default hard failure.
            //
            // Every i64 that crosses this boundary is a SQLite rowid or a count
            // of words, and SQLite's own AUTOINCREMENT ceiling is far below the
            // 2^53 where a JS number stops being exact. The default refuses to
            // guess and is right to, but here the range genuinely is safe and
            // `bigint` would be a lie in the other direction: the values arrive
            // as JSON numbers, so the frontend could never have received a
            // BigInt anyway.
            .bigint(specta_typescript::BigIntExportBehavior::Number);

        // LOGGED, NEVER FATAL. This is a developer convenience that happens to
        // run inside the application; a failure to write it must not stop Echo
        // starting. It did exactly that once — an `.expect` here panicked the
        // app on launch over a TypeScript file the user has no interest in.
        if let Err(e) = builder.export(exporter, "../src/lib/bindings.ts") {
            eprintln!("Could not regenerate src/lib/bindings.ts: {e}");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Hold-to-talk brackets one utterance between the key going
                    // down and coming back up; every other mode acts on the
                    // press alone and ignores the release.
                    let hold = app
                        .try_state::<AppState>()
                        .map(|s| {
                            let conn = s.db.lock().unwrap();
                            storage::repositories::get_setting(&conn, "recording_mode")
                                .unwrap_or(None)
                                .as_deref()
                                == Some("hold")
                        })
                        .unwrap_or(false);

                    let _ = match (hold, event.state) {
                        (false, ShortcutState::Pressed) => app.emit("echo://hotkey-toggle", ()),
                        (true, ShortcutState::Pressed) => app.emit("echo://hotkey-press", ()),
                        (true, ShortcutState::Released) => app.emit("echo://hotkey-release", ()),
                        _ => Ok(()),
                    };
                })
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // No extra argv on an autostart launch: `--selftest`, `--transcribe`
        // and `--benchmark` all exit without ever showing a window, so passing
        // one through here would produce a login that silently does nothing.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Could not resolve app data directory");

            std::fs::create_dir_all(&data_dir)?;
            init_tracing(&data_dir);
            log_panics();
            // Before anything the user can click: on Linux the password-field
            // guard learns focus from events, and misses whatever was focused
            // before it started listening. Returns at once.
            core::field::start();
            let db_path = data_dir.join("echo.db");

            info!("Opening database at {}", db_path.display());
            let conn = db::open(&db_path).expect("Failed to open database");

            let entries = storage::repositories::list_dictionary_entries(&conn)
                .unwrap_or_default()
                .into_iter()
                .map(|e| core::dictionary::DictionaryEntry {
                    id: e.id,
                    phrase: e.phrase,
                    replacement: e.replacement,
                    enabled: e.enabled,
                    profile_id: e.profile_id,
                })
                .collect();

            // Built before the ASR providers: the local engine holds a handle
            // to it so dictionary terms can bias whisper's decoder, not just
            // patch its output afterwards.
            let dictionary = Arc::new(RwLock::new(DictionaryEngine::new(entries)));

            // Same reason: the local engine reads this per utterance to learn
            // which app is being dictated into and what was just said.
            let prompt_ctx = Arc::new(core::asr::prompt::PromptContext::default());

            // Apply the history retention policy at startup, so it also runs
            // for someone who just shortened the window. It runs again after
            // every transcript is written — see `commands::recording` — because
            // Echo is built to stay open for weeks, and a policy that only
            // applies on restart is not a policy.
            match storage::repositories::apply_retention(&conn) {
                Ok(0) => {}
                Ok(n) => info!("Removed {n} transcript(s) past the retention window"),
                Err(e) => tracing::warn!("History retention pass failed: {e}"),
            }

            // Default to the local (offline) Whisper engine on first run.
            let active_provider = storage::repositories::get_setting(&conn, "asr_provider")
                .unwrap_or(None)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "local".into());

            let asr_manager = Arc::new(AsrManager::new(active_provider.clone()));

            let models_dir = data_dir.join("models");
            std::fs::create_dir_all(&models_dir)?;
            let model_manager = Arc::new(ModelManager::new(models_dir));

            // whisper.cpp CLI binary: a copy bundled in the installer's
            // resources is preferred; otherwise it is downloaded on first run
            // or found on PATH.
            let bin_dir = data_dir.join("bin");
            std::fs::create_dir_all(&bin_dir)?;
            let bundled_bin = core::runtime_deps::bundled_whisper_dir(app.handle());
            // Probed once here rather than per transcription: it costs a
            // process spawn, and the answer cannot change while we run.
            let gpu = core::gpu::detect();
            info!("Compute backend: {}", gpu.label());
            let binary_manager = Arc::new(
                BinaryManager::new(bin_dir)
                    .with_bundled_dir(bundled_bin)
                    .with_gpu(gpu),
            );
            let whisper_server = Arc::new(core::asr::whisper_server::WhisperServer::new());

            // The second local engine. Its binaries live beside whisper's, in
            // their own pack directories, and nothing is downloaded until the
            // user picks a model that needs it.
            let nemo_binaries = Arc::new(
                core::asr::nemo::NemoBinaries::new(core::asr::nemo::binaries_dir_of(&data_dir))
                    .with_gpu(gpu),
            );
            let nemo_server = Arc::new(core::asr::nemo_server::NemoServer::new());

            // Selected local model (defaults to base.en).
            let whisper_model = storage::repositories::get_setting(&conn, "whisper_model")
                .unwrap_or(None)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| core::asr::model_manager::DEFAULT_MODEL.to_string());

            // If the local engine is fully provisioned, register it now. Until
            // then `asr_provider` may be "local" with no provider registered —
            // onboarding downloads the binary + model and calls set_asr_provider.
            if let Some(binary) = binary_manager.resolve() {
                if model_manager.is_downloaded(&whisper_model) {
                    let _ = binary; // presence check only; the provider resolves per call
                    let (threads, gpu_allowed) = commands::asr::local_decode_settings(&conn);
                    let provider = commands::asr::build_local_provider(
                        binary_manager.clone(),
                        whisper_server.clone(),
                        model_manager.clone(),
                        &whisper_model,
                        dictionary.clone(),
                        prompt_ctx.clone(),
                        threads,
                        gpu_allowed,
                    );
                    let asr = asr_manager.clone();
                    tauri::async_runtime::block_on(async move {
                        asr.register(Arc::new(provider)).await;
                    });
                }
            }

            // Load the decoder's weights now rather than inside the first
            // dictation. Spawned, so a slow model never delays the window.
            {
                let asr = asr_manager.clone();
                tauri::async_runtime::spawn(async move { asr.preload_active().await });
            }

            // Tell the screen when the engine the user chose stops being the
            // one answering. The diversion is deliberately silent in the
            // transcript — you still get your words, and history still credits
            // the provider you picked — but the pill and the title bar name
            // the active engine, and a name that has quietly stopped being
            // true is worse than no name at all.
            {
                let handle = app.handle().clone();
                let asr = asr_manager.clone();
                tauri::async_runtime::block_on(async move {
                    asr.set_fallback_notify(Arc::new(move |provider: &str| {
                        let _ = handle.emit(
                            "echo://asr-fell-back",
                            serde_json::json!({ "provider": provider }),
                        );
                    }))
                    .await;
                });
            }

            // Wake-word models live beside the Whisper models; nothing is
            // fetched until the user enables the feature.
            let wake_dir = data_dir.join("wake");
            std::fs::create_dir_all(&wake_dir)?;
            let wake_models = Arc::new(core::wake::WakeModelManager::new(wake_dir));

            // Load the Silero VAD model; energy VAD is the fallback on failure.
            // (ONNX Runtime is statically linked into the binary, so there is
            // nothing to bundle or locate for this.)
            let silero = match core::vad::SileroModel::load() {
                Ok(m) => Some(Arc::new(m)),
                Err(e) => {
                    tracing::warn!("Silero VAD unavailable, using energy VAD: {e}");
                    None
                }
            };

            // Register any cloud ASR providers whose API keys are in the keychain.
            // Driven by the catalog so a new provider is one row there, not an
            // edit here that someone will forget.
            for spec in core::asr::catalog::PROVIDERS {
                let Ok(Some(key)) = storage::keychain::get_api_key(spec.id) else {
                    continue;
                };
                match commands::providers::build_provider_with(
                    &conn,
                    dictionary.clone(),
                    prompt_ctx.clone(),
                    spec.id,
                    key,
                ) {
                    Ok(provider) => {
                        let asr = asr_manager.clone();
                        tauri::async_runtime::block_on(async move {
                            asr.register(provider).await;
                        });
                    }
                    // A stored key whose provider will not build — an endpoint
                    // that was never filled in, a region cleared since — must
                    // not be silent. Startup continues; the user gets a reason
                    // in the log instead of a provider that just isn't there.
                    Err(e) => {
                        tracing::warn!(provider = spec.id, "Cloud provider not registered: {e}")
                    }
                }
            }

            // Telemetry (local-only). Mirror the persisted opt-in flag, default on.
            let telemetry_enabled = storage::repositories::get_setting(&conn, "telemetry_enabled")
                .unwrap_or(None)
                .map(|v| v != "false")
                .unwrap_or(true);
            let telemetry = core::telemetry::TelemetryService::new(telemetry_enabled);
            telemetry.record(
                &conn,
                "app_started",
                Some(serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                })),
            );

            // Plugins: ensure the directory exists and load any enabled plugins.
            let plugins_dir = data_dir.join("plugins");
            std::fs::create_dir_all(&plugins_dir)?;
            let mut plugin_loader = core::plugins::loader::PluginLoader::new();
            if let Ok(rows) = storage::repositories::list_plugins(&conn) {
                let ctx = core::plugins::PluginContext {
                    data_dir: plugins_dir.clone(),
                    settings: Arc::new(|_| None),
                };
                for row in rows {
                    if !row.enabled {
                        continue;
                    }
                    let Ok(manifest) =
                        serde_json::from_str::<core::plugins::PluginManifest>(&row.manifest)
                    else {
                        continue;
                    };
                    let name = row.name;
                    let lib = plugins_dir.join(&name).join(&manifest.entry);

                    // A plugin whose library changed since it was installed is
                    // not the one the user agreed to run. Disable rather than
                    // load, so the next launch does not silently try again.
                    match core::plugins::integrity::verify(&lib, row.lib_sha256.as_deref()) {
                        Ok(verdict) if !verdict.is_trusted() => {
                            tracing::error!("{}", verdict.refusal(&name));
                            let _ = storage::repositories::set_plugin_enabled(&conn, &name, false);
                            continue;
                        }
                        Ok(core::plugins::integrity::Verdict::FirstSeen(hash)) => {
                            // Installed before fingerprints existed: adopt what
                            // is there now, and check it from here on.
                            let _ =
                                storage::repositories::set_plugin_fingerprint(&conn, &name, &hash);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("Can't verify plugin '{name}', not loading it: {e}");
                            continue;
                        }
                    }

                    if let Err(e) = plugin_loader.load(&lib, &ctx) {
                        tracing::error!("Failed to load plugin '{name}': {e}");
                    }
                }
            }

            // First run shows the settings window with the onboarding wizard.
            let onboarding_done = storage::repositories::get_setting(&conn, "onboarding_complete")
                .unwrap_or(None)
                .map(|v| v == "true")
                .unwrap_or(false);

            // Same rule as the whisper provider above: register it only when
            // both halves are present, so a half-provisioned engine is never
            // the active one.
            {
                let nemo_model = storage::repositories::get_setting(&conn, "nemo_model")
                    .unwrap_or(None)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| core::asr::model_manager::DEFAULT_NEMO_MODEL.to_string());
                if nemo_binaries.is_installed() && model_manager.is_downloaded(&nemo_model) {
                    let (_, gpu_allowed) = commands::asr::local_decode_settings(&conn);
                    let provider = core::asr::nemo::NemoProvider::new(
                        nemo_binaries.clone(),
                        nemo_server.clone(),
                        model_manager.model_path(&nemo_model),
                    )
                    .with_gpu_allowed(gpu_allowed)
                    .with_dictionary(dictionary.clone())
                    .with_prompt_context(prompt_ctx.clone());
                    let asr = asr_manager.clone();
                    tauri::async_runtime::block_on(async move {
                        asr.register(Arc::new(provider)).await;
                    });
                }
            }

            let app_state = AppState {
                exclusive: Default::default(),
                db: Mutex::new(conn),
                audio: Arc::new(AudioService::new().expect("Failed to initialize audio")),
                asr: asr_manager,
                models: model_manager,
                nemo_binaries,
                nemo_server,
                binaries: binary_manager,
                whisper_server,
                silero,
                wake_models,
                wake_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                dictionary,
                prompt_ctx,
                injector: Arc::from(platform_injector()),
                telemetry,
                plugins: Mutex::new(plugin_loader),
                plugins_dir,
                dictation: Mutex::new(Default::default()),
                cancel_generation: Default::default(),
                last_stop_at: Mutex::new(None),
                last_delivery: Mutex::new(None),
                last_utterance: Arc::new(Mutex::new(None)),
                modtap: Mutex::new(None),
            };

            app.manage(app_state);

            // The dictionary engine and the ASR engines were built before any
            // plugin loaded, so enabled plugins' entries and engines join now.
            {
                let handle = app.handle().clone();
                tauri::async_runtime::block_on(async move {
                    commands::plugins::sync_capabilities(&handle.state::<AppState>()).await;
                });
            }

            // Bind the global hotkey now that state is available. Which
            // mechanism gets used depends on the shortcut itself.
            let handle = app.handle().clone();
            if let Err(e) = commands::hotkey::apply(&handle, &handle.state::<AppState>()) {
                tracing::warn!("Couldn't bind the global hotkey: {e}");
            }

            // Drain the egress channel into the database. Requests are logged
            // fire-and-forget so a network call never waits on SQLite.
            {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<core::egress::Egress>();
                core::egress::init(tx);

                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Trim occasionally rather than on every insert; this is a
                    // rolling log, not an audit trail.
                    let mut since_trim = 0u32;
                    while let Some(e) = rx.recv().await {
                        let state = handle.state::<AppState>();
                        let conn = state.db.lock().unwrap();
                        if let Err(err) =
                            storage::repositories::insert_egress(&conn, &e.host, &e.purpose)
                        {
                            tracing::warn!("Failed to log egress: {err}");
                        }
                        since_trim += 1;
                        if since_trim >= 50 {
                            since_trim = 0;
                            let _ = storage::repositories::trim_egress(&conn, 1000);
                        }
                    }
                });
            }

            // Arm the wake-word listener if the user enabled it last session.
            commands::wake::rearm(app.handle());

            // Dictionary sync through a shared folder. Does nothing until the
            // user picks a folder and switches it on.
            commands::dictionary::start_sync(app.handle().clone());

            // `--selftest` runs here rather than in main(): the point is to
            // exercise a real startup, and everything worth checking — the
            // database, the audio router, the ASR provider — only exists once
            // setup has run. It never returns.
            if selftest::requested() {
                selftest::run(app.handle());
            }
            // `--transcribe <file>` prints a transcript and exits, so Echo can
            // sit in the middle of a shell pipeline. Same placement and the
            // same reason as the selftest above: everything it needs — the
            // database, the model, the engine — only exists once setup has run.
            if cli::requested() {
                cli::run(app.handle());
            }
            if benchmark::requested() {
                benchmark::run(app.handle());
            }

            // The tray is the only persistent route back to Settings and to
            // Quit, because the pill has no chrome and stays out of the
            // taskbar. Degraded rather than fatal: a desktop with no status
            // area can still dictate, and refusing to start would be the
            // worse trade.
            // A spool still on disk means the last session did not end the way
            // it should have. Before the tray, so a launch that fails later
            // has still rescued the audio.
            match crate::core::spool::recover(&data_dir) {
                Ok(Some(path)) => {
                    tracing::warn!(
                        "Recovered audio from an interrupted session: {}",
                        path.display()
                    )
                }
                Ok(None) => {}
                Err(e) => tracing::error!("Could not recover the interrupted session: {e}"),
            }

            if let Err(e) = tray::init(app.handle()) {
                tracing::warn!("Tray icon unavailable, Echo is reachable only via the pill: {e}");
            }

            // Echo draws its own title bar (see components/common/TitleBar.tsx),
            // because the native one on Windows and Linux arrives in the OS's
            // colours and reads as a strip of another app stapled to the top of
            // a very dark window. macOS is left decorated on purpose: its
            // traffic lights are muscle memory, and `titleBarStyle: "Overlay"`
            // floats them inside our own strip.
            //
            // Done here rather than with `decorations: false` in the config
            // because that key has no per-platform form — splitting it out into
            // tauri.macos.conf.json would mean duplicating the whole `windows`
            // array, which the merge replaces wholesale, and watching the two
            // copies drift.
            //
            // Note for anyone changing the window size: this reclaims the
            // caption strip into the client area while the outer size stays
            // put, so the window renders ~30px taller than `height` in
            // tauri.conf.json says. The config number is picked so the result
            // still fits the ~720px work area of a 1366x768 laptop — overshoot
            // it and `center: true` places the top at a negative Y, putting our
            // own title bar (and its close button) off the top of the screen,
            // with no native chrome left to recover from.
            #[cfg(not(target_os = "macos"))]
            if let Some(win) = app.get_webview_window("main") {
                // Safe to do before the window is shown (`visible: false` in the
                // config), so there is no frame to flicker away.
                let _ = win.set_decorations(false);
            }

            // The native material behind the glass.
            //
            // The web layer only ever contributes the tint, the hairline and
            // the inner highlight — see src/styles/tokens.css §Materials. This
            // is the part it is drawn ON, and it is the difference between
            // real glass and a CSS imitation of it.
            //
            // Failure is ignored on purpose, per platform: Mica needs Windows
            // 11 and returns an error on 10, and there is no compositor
            // material to ask for on most Linux desktops. In both cases
            // `backdrop-filter` in the stylesheet is already the fallback, so
            // the window is correct either way and an error here would be
            // noise about a feature the user cannot act on.
            if let Some(win) = app.get_webview_window("main") {
                #[cfg(target_os = "windows")]
                let applied = window_vibrancy::apply_mica(&win, None).is_ok();
                #[cfg(target_os = "macos")]
                let applied = window_vibrancy::apply_vibrancy(
                    &win,
                    window_vibrancy::NSVisualEffectMaterial::Sidebar,
                    None,
                    None,
                )
                .is_ok();
                #[cfg(target_os = "linux")]
                let applied = false;

                // THE WINDOW IS TRANSPARENT, so something has to be behind the
                // content. When the material took, that something is the OS and
                // the page stays translucent; when it did not — Windows 10, most
                // Linux desktops — the page must paint an opaque surface itself
                // or the app is literally see-through.
                //
                // Told to the page rather than guessed by it: whether Mica
                // applied is knowable only here, and a stylesheet that assumed
                // either answer would be wrong on half the machines Echo runs
                // on.
                if applied {
                    let _ = win
                        .eval("document.documentElement.setAttribute('data-material', 'native')");
                }
            }

            // Surface the settings window on first launch so onboarding can run.
            if !onboarding_done {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(builder.invoke_handler())
        .build(tauri::generate_context!())
        .expect("error while building echo")
        .run(|app, event| {
            // The resident whisper model is a child process holding a few
            // hundred megabytes. `AppHandle::exit` — which the Quit button
            // calls — ends the process without running destructors, so
            // `kill_on_drop` never fires and the server is orphaned: it
            // outlives Echo, keeps the model in RAM, and accumulates one copy
            // per launch.
            //
            // This covers an orderly exit. A crash or a force-kill runs none of
            // it; that case is handled where the server is spawned, by the OS
            // (see `whisper_server::spawn_contained`). This stays regardless:
            // macOS has no such mechanism, and there a clean exit is the only
            // thing that stops the server before Echo's next launch sweeps it.
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<AppState>() {
                    tauri::async_runtime::block_on(state.whisper_server.shutdown());
                }
            }
        });
}

/**
 * SOURCE OF TRUTH KEYWORDS: ipc_builder, collect_commands, export_bindings
 * WHAT:  Every command Echo exposes, collected once so both the runtime handler
 *        and the generated TypeScript come from the same list.
 * WHY:   THE BINDINGS USED TO BE HAND-WRITTEN. `src/ipc/commands.ts` was 565
 *        lines of interfaces mirroring Rust types with nothing enforcing that
 *        they still matched — rename a field in Rust and the only thing that
 *        noticed was a user, at runtime, with an undefined where a value
 *        should be. One list means the drift is not possible rather than
 *        merely unlikely.
 * WHERE: Consumed by `run` for the invoke handler, and by the export test in
 *        this file that writes src/lib/bindings.ts.
 */
fn ipc_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        commands::app::quit,
        commands::app::get_autostart,
        commands::app::set_autostart,
        commands::app::account_name,
        commands::audio::get_audio_devices,
        commands::audio::test_input_level,
        commands::asr::list_models,
        commands::asr::download_nemo_engine,
        commands::asr::nemo_status,
        commands::asr::download_model,
        commands::asr::set_asr_provider,
        commands::asr::set_whisper_model,
        commands::asr::delete_model,
        commands::asr::whisper_ready,
        commands::asr::download_whisper_binary,
        commands::asr::gpu_status,
        commands::asr::download_gpu_pack,
        commands::asr::set_gpu_enabled,
        commands::asr::set_whisper_threads,
        commands::asr::installed_packs,
        commands::import::transcribe_file,
        commands::import::supported_import_formats,
        commands::recording::start_recording,
        commands::recording::stop_recording,
        commands::recording::is_recording,
        commands::recording::warm_microphone,
        commands::dictionary::list_dictionary,
        commands::dictionary::add_dictionary_entry,
        commands::dictionary::delete_dictionary_entry,
        commands::dictionary::toggle_dictionary_entry,
        commands::dictionary::export_dictionary,
        commands::dictionary::import_dictionary,
        commands::dictionary::learn_from_correction,
        commands::dictionary::sync_dictionary_now,
        commands::dictionary::get_dictionary_sync_status,
        commands::dictionary::list_snippets,
        commands::dictionary::save_snippet,
        commands::dictionary::delete_snippet,
        commands::history::get_history,
        commands::history::clear_history,
        commands::history::get_dictation_stats,
        commands::history::get_insights,
        commands::injection::check_accessibility_permission,
        commands::injection::inject_text,
        commands::injection::secure_field_detection,
        commands::hotkey::get_hotkey,
        commands::hotkey::register_hotkey,
        commands::hotkey::hotkey_support,
        commands::hotkey::set_recording_mode,
        commands::hotkey::set_fixup_hotkey,
        commands::hotkey::get_fixup_hotkeys,
        commands::fixup::undo_last_insert,
        commands::fixup::retry_last,
        commands::fixup::retry_targets,
        commands::providers::set_api_key,
        commands::providers::get_api_key_set,
        commands::providers::remove_api_key,
        commands::providers::list_cloud_providers,
        commands::providers::set_provider_setting,
        commands::providers::test_api_key,
        commands::telemetry::get_telemetry_summary,
        commands::telemetry::clear_telemetry,
        commands::telemetry::set_telemetry_enabled,
        commands::telemetry::record_telemetry_event,
        commands::plugins::list_plugins,
        commands::plugins::inspect_plugin,
        commands::plugins::install_plugin,
        commands::plugins::enable_plugin,
        commands::plugins::disable_plugin,
        commands::plugins::uninstall_plugin,
        commands::plugins::scaffold_plugin,
        commands::wake::set_wake_word_enabled,
        commands::wake::set_wake_word_model,
        commands::wake::set_wake_word_sensitivity,
        commands::wake::list_wake_words,
        commands::wake::download_wake_model,
        commands::wake::import_wake_model,
        commands::wake::wake_word_ready,
        commands::wake::wake_word_active,
        commands::wake::wake_word_status,
        commands::history::export_history,
        commands::profiles::get_foreground_app,
        commands::profiles::list_app_profiles,
        commands::profiles::save_app_profile,
        commands::profiles::delete_app_profile,
        commands::profiles::list_profiles,
        commands::profiles::add_profile,
        commands::profiles::delete_profile,
        commands::profiles::set_dictionary_entry_profile,
        commands::egress::get_egress_log,
        commands::egress::clear_egress_log,
        commands::egress::get_egress_status,
        commands::settings::get_setting,
        commands::settings::settings_snapshot,
        commands::settings::set_setting,
        commands::settings::spoken_punctuation_languages,
        commands::settings::number_languages,
        commands::settings::cleanup_languages,
        commands::settings::dictation_languages,
        commands::app::diagnostics,
        commands::app::open_log,
        commands::app::recovered_recordings,
        commands::app::discard_recovered,
        commands::audio::silero_available,
    ])
}

#[cfg(test)]
mod bindings_export {
    /*!
     * SOURCE OF TRUTH KEYWORDS: every_command_is_in_the_bindings, bindings.ts
     * WHAT:  Checks the committed src/lib/bindings.ts still names every command
     *        the crate registers.
     * WHY:   The bindings are WRITTEN by a debug run (see `run`), not by this
     *        test, because instantiating Wry in a test binary kills the process
     *        at load — see the comment there. So this is the half that CAN run
     *        without a runtime: it reads the command list out of this file's own
     *        source and asserts each name appears in the generated output.
     *
     *        Crude, and crude is the point. It cannot check types, but it
     *        catches the failure that actually happens — someone adds a command
     *        and commits without running the app — and it fails for a reason
     *        anyone can check in ten seconds.
     */

    /// Command names, read out of the `collect_commands!` block in this file.
    fn registered_commands() -> Vec<String> {
        let source = include_str!("lib.rs");
        let start = source
            .find("collect_commands![")
            .expect("the collect_commands block moved");
        let end = source[start..].find("])").expect("unterminated block") + start;
        source[start..end]
            .lines()
            .filter_map(|line| {
                let line = line.trim().trim_end_matches(',');
                line.strip_prefix("commands::")
                    .and_then(|rest| rest.split("::").nth(1))
                    .map(str::to_string)
            })
            .collect()
    }

    #[test]
    fn every_command_is_in_the_bindings() {
        let commands = registered_commands();
        assert!(
            commands.len() > 50,
            "only found {} commands; the parser above has stopped matching the              collect_commands block rather than the app having shrunk",
            commands.len()
        );

        let bindings = match std::fs::read_to_string("../src/lib/bindings.ts") {
            Ok(text) => text,
            // A fresh checkout has not run the app yet. Not a failure — there is
            // nothing stale to catch.
            Err(_) => return,
        };

        let missing: Vec<_> = commands
            .iter()
            .filter(|name| !bindings.contains(name.as_str()))
            .collect();

        assert!(
            missing.is_empty(),
            "these commands are registered but absent from the generated bindings:
  {:?}
             Run the app once in debug (`npm run tauri dev`) to regenerate              src/lib/bindings.ts, and commit the result.",
            missing
        );
    }
}
