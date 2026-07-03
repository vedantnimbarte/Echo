# Bundled resources

Files here are copied into the packaged app's resource directory by the Tauri
bundler (see the `bundle.resources` map in `tauri.conf.json`).

- `silero_vad.onnx` — the Silero VAD model (also `include_bytes!`-embedded in
  the binary; kept here for reference).
- `bin/` — **staged at package time.** The whisper.cpp CLI and its sidecar DLLs.
  Empty in the repo apart from `.gitkeep`; a release build fills it via
  `scripts/stage-runtime-deps.mjs`. Consumed by
  `core::runtime_deps::bundled_whisper_dir` → `BinaryManager::with_bundled_dir`.

The `.gitkeep` keeps the (otherwise empty) `bin/` dir present so the
`bundle.resources` glob matches in dev builds too. See `docs/BUNDLING.md`.

> The ONNX Runtime that backs Silero VAD is **statically linked** into the
> executable by `ort`, so there is no shared library to ship here.
