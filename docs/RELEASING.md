# Releasing Echo

Tagging a commit with `v*` (e.g. `v0.1.0`) triggers
`.github/workflows/release.yml`, which builds Windows / macOS (universal) / Linux
installers, stages the offline `whisper-cli` into each bundle, and creates a
**draft** GitHub Release with the artifacts.

A second job then checksums every artifact, attaches `SHA256SUMS.txt` (the
install scripts verify against it), and commits the filled-in packaging
manifests back to `main`.

## Auto-update is currently OFF

`bundle.createUpdaterArtifacts` is **false** and `plugins.updater.pubkey` is
empty, so releases ship no `latest.json` and installed copies never check for
updates. Users upgrade by re-running the install script.

This is deliberate: with `createUpdaterArtifacts` on and no key, `tauri build`
fails outright. Turning updates on is the one-time setup below.

## Turning auto-update on

The updater only installs builds it can cryptographically verify, so it needs a
signing keypair.

1. Generate a keypair (keep the password somewhere safe):

   ```bash
   cd echo-app
   npm run tauri signer generate -- -w ~/.tauri/echo-updater.key
   ```

   It prints a **public key** and writes the **private key** to that path.

2. Paste the public key into `echo-app/src-tauri/tauri.conf.json`:

   ```jsonc
   "plugins": { "updater": { "pubkey": "<PASTE PUBLIC KEY HERE>" } }
   ```

3. Add two GitHub repo secrets (Settings → Secrets and variables → Actions):

   | Secret | Value |
   |---|---|
   | `TAURI_SIGNING_PRIVATE_KEY` | contents of `~/.tauri/echo-updater.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password from step 1 |

4. Flip `bundle.createUpdaterArtifacts` back to `true` in `tauri.conf.json`.

Commit the pubkey and the flag together; releases from then on sign updates and
the app checks on launch. Installs made *before* this change won't auto-update
to it — they have no public key to verify against — so those users reinstall
once.

> Never commit the private key. If it leaks, generate a new pair and ship a
> release with the new pubkey (older installs won't auto-update to it and must be
> reinstalled once).

## Cutting a release

Bump the version in **three** places — they must agree or the packaging
manifests will point at files that don't exist:

- `echo-app/src-tauri/tauri.conf.json` → `version`
- `echo-app/src-tauri/Cargo.toml` → `version`
- `echo-app/package.json` → `version`

Then:

```bash
git tag v0.1.0
git push origin v0.1.0
```

What happens next:

1. **`build`** — three runners produce installers and attach them to a *draft*
   release.
2. **`manifests`** — downloads those assets, writes `SHA256SUMS.txt` and uploads
   it, fills in the winget/Homebrew/snap manifests, and commits them to `main`.
3. **You** — review the draft, then publish:

   ```bash
   gh release view v0.1.0            # check every expected asset is attached
   gh release edit v0.1.0 --draft=false
   ```

Publishing is manual on purpose: the install scripts point at *latest*, so
publishing is the moment a build becomes what users get. Check the assets first.

### Verifying a release actually installs

The scripts are the user's first experience, so exercise them, not just the
build log:

```bash
# macOS/Linux
ECHO_VERSION=v0.1.0 sh scripts/install.sh

# Windows
./scripts/install.ps1 -Version v0.1.0
```

A failure here usually means an asset name changed — the scripts match on
`.dmg` / `.AppImage` / `-setup.exe`, and `packaging/homebrew/echo.rb` builds its
URL from `Echo_#{version}_universal.dmg`.

## Platform coverage

| Platform | Built | Notes |
|---|---|---|
| Windows x64 | ✅ | NSIS `.exe` + `.msi` |
| Linux x86_64 | ✅ | `.AppImage`, `.deb`, `.rpm` |
| macOS arm64 | ✅ | `.dmg` |
| macOS x86_64 | ❌ | `ort` ships no prebuilt ONNX Runtime for `x86_64-apple-darwin` (see `ort-sys`'s `build/download/dist.txt`, which lists `aarch64-apple-darwin` alone). A universal build fails at link time. Restoring Intel support means compiling ONNX Runtime from source and linking `ort` against it. |
| Linux aarch64 | ❌ | Not built yet; `ort` does support the target. |

## Code signing (OS-level, separate from updater signing)

The updater keypair proves *"this update came from us."* It does **not** make the
OS trust the installer — that needs an Apple Developer cert (macOS notarization)
and an Authenticode cert (Windows). Those are not configured yet, so users see
the first-run warnings documented in the README's **Installing** section. Wiring
them is optional for an open-source launch; see
<https://tauri.app/distribute/sign/>.
