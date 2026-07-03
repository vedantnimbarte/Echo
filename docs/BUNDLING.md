# Bundling the whisper.cpp CLI

Echo's default local ASR engine shells out to the **whisper.cpp CLI**
(`whisper-cli`). By default `BinaryManager` downloads it on first launch, which
means a freshly installed app needs network access before local transcription
works. This lets a release **ship whisper-cli inside the installer** so it works
fully offline, while keeping the download path as a fallback.

## What about ONNX Runtime?

An earlier draft also tried to bundle the ONNX Runtime shared library (for
Silero VAD). **Validation showed that is unnecessary.** The `ort` build with
`download-binaries` emits:

```
cargo:rustc-link-lib=static=onnxruntime
```

i.e. ONNX Runtime is **statically linked into `echo.exe`** — there is no
`onnxruntime.dll`/`.dylib`/`.so` to ship, and no runtime path to resolve. The
`silero_loads_and_runs_inference_without_external_runtime` test in
`core/vad/silero.rs` builds a session and runs inference with no external
library present, locking in that property.

(The previously-proposed `ort` `load-dynamic` feature was **dropped** — it would
have replaced the working static link with a dependency on an external DLL that
`download-binaries` does not provide, silently regressing Silero to the energy
VAD fallback.)

## How it works

### Resolution (runtime) — `core::runtime_deps`

`bundled_whisper_dir(app)` returns `<resource_dir>/bin` if it exists;
`BinaryManager::with_bundled_dir` then prefers a `whisper-cli` found there over
downloading. Resolution order is **bundled → downloaded → PATH**. Additive and
backward-compatible: with an empty `bin/` the app behaves exactly as before.

### Packaging (build time)

- `tauri.conf.json` maps `resources/bin/` into the bundle's resource dir under
  `bin/`.
- `resources/bin/` is committed empty (just a `.gitkeep`) so the resource glob
  matches in dev builds without shipping anything real.
- A release job runs `scripts/stage-runtime-deps.mjs` **before** `tauri build`
  to download and flatten whisper-cli (+ its DLLs) into `resources/bin/`.

### Proposed release step

Add to `.github/workflows/release.yml`, before the `tauri-action` step:

```yaml
      - name: Stage bundled whisper-cli
        if: matrix.platform == 'windows-latest'
        run: node scripts/stage-runtime-deps.mjs
        working-directory: echo-app
```

(Not committed here — it should land together with a validated packaged build.)

## Validation status

Done on a Windows dev host (this is what turned the earlier speculative draft
into the current shape):

- [x] `ort` statically links ONNX Runtime (`rustc-link-lib=static=onnxruntime`
      in the build output); Silero loads + runs inference with no external
      runtime present — `silero_loads_and_runs_inference_without_external_runtime`.
      → **onnxruntime bundling dropped entirely.**
- [x] `scripts/stage-runtime-deps.mjs` runs end-to-end: downloads
      `whisper-bin-x64.zip`, extracts (via PowerShell `Expand-Archive` — GNU
      `tar` from git-bash misparses `C:\` paths), and stages a curated set.
- [x] Fixed a real bug: the pinned `v1.7.4` tag ships **no** assets (404) — both
      the staging script and `binary_manager.rs` now use `v1.7.6`.
- [x] Curated the bundle to `whisper-cli.exe` + `ggml*.dll` + `whisper.dll`
      (dropped ~15 unused tools + `SDL2.dll`); **ran the staged `whisper-cli.exe`
      against exactly that set to confirm its DLLs resolve.**
- [x] `BinaryManager::with_bundled_dir` prefers a staged binary — unit test.
- [x] `cargo test` green; `tauri.conf.json` passes tauri-build schema validation.

Still needs a real packaged build / other platforms:

- [ ] `tauri build` actually places `resources/bin/*` at `<resource_dir>/bin/`
      and `resource_dir()` resolves there (verify on Windows first).
- [ ] The staged set includes every DLL whisper-cli needs at runtime
      (`ggml*.dll`, etc.).
- [ ] Code signing / notarization covers the bundled `.exe`/`.dll`.
- [ ] macOS/Linux policy: upstream ships no prebuilt CLI, so those platforms
      keep the PATH fallback unless we build whisper.cpp in CI.
