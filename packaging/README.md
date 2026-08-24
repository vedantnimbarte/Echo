# Packaging & package-manager submissions

Every file here is regenerated per release by `scripts/update-manifests.mjs`,
which fills in versions, URLs and SHA256s from the actual release assets. Don't
hand-edit the version or hash fields — they'll be overwritten.

| Channel | Status | Path |
|---|---|---|
| **winget** | Ready to submit | `winget/manifests/e/Echo/Echo/<version>/` |
| **Homebrew** | Ship as your own tap — see below | `homebrew/echo.rb` |
| Flatpak | Template, unsubmitted | `flatpak/com.echo.app.yml` |
| Snap | Template, unsubmitted | `snap/snapcraft.yaml` |

---

## winget

The three-file manifest set is complete and mirrors the exact directory layout
`microsoft/winget-pkgs` expects, so it can be copied straight across.

**Submitting:**

```sh
# 1. Fork and clone microsoft/winget-pkgs
gh repo fork microsoft/winget-pkgs --clone
cd winget-pkgs

# 2. Copy the manifests in (the layout already matches)
cp -r /path/to/Echo/packaging/winget/manifests/e/Echo manifests/e/

# 3. Validate and test locally — do this on a real Windows machine
winget validate --manifest manifests/e/Echo/Echo/0.1.0
winget install --manifest manifests/e/Echo/Echo/0.1.0

# 4. One package version per pull request
git checkout -b Echo.Echo-0.1.0
git add manifests/e/Echo/Echo/0.1.0
git commit -m "New package: Echo.Echo version 0.1.0"
git push -u origin Echo.Echo-0.1.0
gh pr create --repo microsoft/winget-pkgs
```

Manifests can be checked against the published schemas before submitting,
without Windows:

```sh
pip install jsonschema pyyaml
# validate each file against https://aka.ms/winget-manifest.<type>.1.12.0.schema.json
```

Automated validation runs on the PR. Things it checks that are easy to get
wrong: the SHA256 must match the asset byte-for-byte, the installer URL must be
publicly reachable, and the installer must support a silent install (Tauri's
NSIS installer does — `/S`).

`winget validate` and `winget install --manifest` need Windows. **Neither has
been run against these files**, so run both before opening the PR.

### Notes on specific fields

- `PackageIdentifier: Echo.Echo` — `manifests/e/Echo` was unclaimed at the time
  of writing, and `Echo` matches `bundle.publisher` in `tauri.conf.json`, which
  is what the installer writes into the registry. If a reviewer objects to
  claiming the generic `Echo` publisher namespace, `vedantnimbarte.Echo` is the
  conventional fallback for an individual developer.
- `InstallerType: nullsoft` — the schema enum spells NSIS as `nullsoft`;
  `nsis` fails validation. Confirmed the installer really is NSIS by finding the
  `Nullsoft` marker in the binary.
- `Scope: user` — Tauri's NSIS installer defaults to a per-user install.
- **`ProductCode` is deliberately absent.** For NSIS this is the uninstall
  registry key, which can't be known without installing on Windows. A wrong
  value silently breaks upgrade detection, so it's better omitted; add it once
  you can read the real key from `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall`.

---

## Homebrew

**Do not submit this to `Homebrew/homebrew-cask` yet — it will be rejected.**

Homebrew enforces a notability threshold, and it is deliberately higher for
people submitting their own project:

| | Forks | Watchers | Stars |
|---|---|---|---|
| General submission | 30 | 30 | 75 |
| **Self-submitted** | 90 | 90 | **225** |

Echo does not currently meet these. The rationale, from Homebrew's own docs, is
that unpopular software attracts less attention and its cask rots — so this is a
maintenance policy, not a judgement of the software.

### Ship a tap instead

A tap is just a GitHub repo named `homebrew-<something>`. It works today, needs
nobody's approval, and the cask file is identical.

```sh
# 1. Create a repo named exactly `homebrew-echo`
gh repo create vedantnimbarte/homebrew-echo --public \
  --description "Homebrew tap for Echo"

# 2. Add the cask at Casks/echo.rb
git clone https://github.com/vedantnimbarte/homebrew-echo
cd homebrew-echo && mkdir -p Casks
cp /path/to/Echo/packaging/homebrew/echo.rb Casks/echo.rb
git add . && git commit -m "Add Echo cask" && git push
```

Users then install with:

```sh
brew install --cask vedantnimbarte/echo/echo
```

Keep `packaging/homebrew/echo.rb` as the source of truth — it's what
`update-manifests.mjs` writes to — and copy it into the tap each release, or
have the tap pull from it.

Once Echo clears the thresholds above, the same file can be submitted to
`homebrew-cask` unchanged.

### Verifying the cask

```sh
brew style --fix packaging/homebrew/echo.rb
brew audit --cask --new packaging/homebrew/echo.rb
```

Neither has been run against this file — both need macOS with Homebrew.
