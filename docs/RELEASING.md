# Releasing Echo

Tagging a commit with `v*` (e.g. `v0.1.0`) triggers
`.github/workflows/release.yml`, which builds Windows / macOS (universal) / Linux
installers, stages the offline `whisper-cli` into each bundle, and creates a
**draft** GitHub Release with the artifacts and an auto-update `latest.json`.

## One-time setup: auto-update signing keys

The in-app updater only ships builds it can cryptographically verify, so the
first release needs a signing keypair. **Do this before your first `v*` tag** —
without it, `tauri build` fails because `plugins.updater.pubkey` is empty and
`createUpdaterArtifacts` is on.

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

That's it — commit the pubkey change; releases now sign updates and the app
auto-checks on launch.

> Never commit the private key. If it leaks, generate a new pair and ship a
> release with the new pubkey (older installs won't auto-update to it and must be
> reinstalled once).

## Cutting a release

```bash
# bump version in echo-app/src-tauri/tauri.conf.json AND package.json, then:
git tag v0.1.0
git push origin v0.1.0
```

Wait for the matrix to finish, review the **draft** release, then publish it.
Publishing makes `latest.json` reachable at the endpoint in `tauri.conf.json`, so
existing installs see the update.

## Code signing (OS-level, separate from updater signing)

The updater keypair proves *"this update came from us."* It does **not** make the
OS trust the installer — that needs an Apple Developer cert (macOS notarization)
and an Authenticode cert (Windows). Those are not configured yet, so users see
the first-run warnings documented in the README's **Installing** section. Wiring
them is optional for an open-source launch; see
<https://tauri.app/distribute/sign/>.
