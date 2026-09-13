# Releasing Echo

Tagging a commit with `v*` (e.g. `v0.1.0`) triggers
`.github/workflows/release.yml`, which builds Windows / macOS arm64 / Linux (x86_64 and arm64)
installers, stages the offline `whisper-cli` into each bundle, and creates a
**draft** GitHub Release with the artifacts.

A second job then checksums every artifact, attaches `SHA256SUMS.txt` (the
install scripts verify against it), and commits the filled-in packaging
manifests back to `main`.

## Auto-update is ON

`bundle.createUpdaterArtifacts` is **true** and `plugins.updater.pubkey` holds
the public key, so every release attaches `.sig` files and a `latest.json`, and
installed copies check `releases/latest/download/latest.json` on launch.

**The two `TAURI_SIGNING_PRIVATE_KEY*` repository secrets must exist before a
tag is pushed.** With `createUpdaterArtifacts` on and no key, `tauri build`
fails outright on every platform. Installs of v0.4.0 and earlier have no public
key and must reinstall once.

The private key and its password live outside the repository, at
`~/.tauri/echo-updater.key` and `~/.tauri/echo-updater.password` on the machine
that generated them. **Back both up.** Lose them and no installed copy can ever
be updated again. Every user has to reinstall onto a build with a new key.

## Setting up auto-update from scratch (done once, kept for key rotation)

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

Commit the pubkey and the flag in one commit; releases from then on sign updates and
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
| Linux aarch64 | ✅ | `.AppImage`, `.deb`, `.rpm`, built on a native `ubuntu-24.04-arm` runner. Compiled and unit-tested in CI; not driven by hand. |

## Code signing (OS-level, separate from updater signing)

The updater keypair proves *"this update came from us."* It does **not** make the
OS trust the installer — that needs an Apple Developer cert (macOS notarization)
and an Authenticode cert (Windows).

**The workflow already reads the secrets.** `release.yml` forwards every variable
`tauri-action` needs for both platforms, so adding a certificate is a matter of
creating repository secrets with no workflow change.

> An earlier version of this page claimed the variables could sit in the build
> step's `env:` and that signing would be skipped when the secrets were unset.
> That is wrong, and it broke the v0.3.0 macOS build. A missing secret
> interpolates to an **empty string**, and the environment variable is still
> defined; Tauri's bundler decides to sign on whether `APPLE_CERTIFICATE` is
> present, not on whether it contains anything. With no secrets configured at
> all, the macOS job compiled for eight minutes and then died in `security
> import` with an empty certificate — producing no `.dmg`, which in turn skipped
> the checksum job and left every platform's installer unverifiable.
>
> The fix is the *Enable code signing* step, which writes each value to
> `$GITHUB_ENV` only when it is non-empty — that is what makes "unset"
> expressible. Do not move these back into the build step's `env:` block: it
> reintroduces a failure that only appears on a tagged release.

| Secret | Platform | What it is |
|---|---|---|
| `APPLE_CERTIFICATE` | macOS | base64 of the Developer ID `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | macOS | its export password |
| `APPLE_SIGNING_IDENTITY` | macOS | e.g. `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` | macOS | the Apple ID, for notarytool |
| `APPLE_PASSWORD` | macOS | an **app-specific** password, not the account one |
| `APPLE_TEAM_ID` | macOS | the 10-character team id |
| `WINDOWS_CERTIFICATE` | Windows | base64 of the Authenticode `.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD` | Windows | its export password |

**Do macOS first.** The two platforms fail differently and the difference
matters. SmartScreen *warns* and still offers "More info → Run anyway", which an
open-source project can live with. Gatekeeper *refuses*, offering only "Move to
Trash" — so on macOS signing is a precondition for being installable at all, and
it additionally gates a Homebrew cask and a Flathub submission. An Apple
Developer account is $99/yr; an OV Authenticode certificate is roughly
$200-600/yr and earns SmartScreen reputation on wall-clock time, which is an
argument for buying it early rather than urgently.

See <https://tauri.app/distribute/sign/>.
