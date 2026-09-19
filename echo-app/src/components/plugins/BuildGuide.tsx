import { useState } from "react";
import { Check, Copy, FolderPlus, ShieldAlert } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";

import { commands } from "../../ipc/commands";
import { Group } from "../common/Page";
import { errorMessage } from "../../lib/errors";

/**
 * How to write an Echo plugin, in the app rather than in a file on GitHub.
 *
 * Everything here is the real contract, checked against the code rather than
 * described from memory: the snippets are the same template
 * `core/plugins/scaffold.rs` writes, and a test in that module fails if the two
 * drift apart. The example it produces is a workspace member, so a snippet that
 * stops compiling fails the build.
 *
 * It is deliberately specific about when each capability runs, because that is
 * the part a trait signature cannot tell you and the part a plugin's design
 * hangs on: an output hook that runs after typing cannot change the text, and
 * an audio hook that runs on every chunk cannot afford to be slow. The order
 * here is the order in `core/plugins/dispatch.rs`.
 */

/** A copyable block of code. The copy button is why this is not a bare `pre`. */
function Code({ children }: { children: string }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    await navigator.clipboard.writeText(children);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="group relative">
      <pre className="overflow-x-auto rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] p-3 pr-12 font-mono text-[12.5px] leading-relaxed text-[var(--ink)]">
        {children}
      </pre>
      <button
        onClick={copy}
        // Always in the tree, not only on hover: a control that exists only for
        // a mouse is one a keyboard can never reach.
        className="absolute right-2 top-2 rounded-md p-1.5 text-[var(--ink-faint)] transition-colors hover:bg-[var(--surface-2)] hover:text-[var(--ink)] focus-visible:text-[var(--ink)]"
        aria-label={copied ? "Copied" : "Copy to clipboard"}
      >
        {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
      </button>
    </div>
  );
}

/**
 * One numbered step.
 *
 * Numbered because this genuinely is a sequence — you cannot write the manifest
 * before the crate exists, or install before you build — and the number is the
 * one piece of information a heading alone would not carry: how far along you
 * are, and whether you have missed one.
 */
function Step({
  n,
  title,
  children,
}: {
  n: number;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex gap-3.5">
      <span
        aria-hidden="true"
        className="mt-px flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-[var(--hairline-strong)] text-[12px] tabular text-[var(--ink-muted)]"
      >
        {n}
      </span>
      <div className="min-w-0 flex-1 space-y-2.5">
        <h4 className="text-[14.5px] font-medium text-[var(--ink)]">{title}</h4>
        {children}
      </div>
    </section>
  );
}

/** Body text of a step. One measure, one size, everywhere on the page. */
function P({ children }: { children: React.ReactNode }) {
  return (
    <p className="max-w-[62ch] text-[13.5px] leading-relaxed text-[var(--ink-muted)]">
      {children}
    </p>
  );
}

const CARGO_TOML = `[package]
name = "my-plugin"
version = "0.1.0"
edition = "2021"

# Echo loads a plugin with dlopen/LoadLibrary, so it has to be a C-ABI dynamic
# library. A plain Rust \`lib\` cannot be loaded at runtime.
[lib]
crate-type = ["cdylib"]

[dependencies]
echo-sdk = "0.2"`;

const LIB_RS = `use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use echo_sdk::{
    export_plugin, OutputPlugin, Plugin, PluginContext, PluginError, PluginResult, Transcript,
};

#[derive(Default)]
struct MyPlugin {
    // Hooks take &self because Echo calls them from more than one thread, so
    // anything learned after creation goes in a cell like this one.
    log: OnceLock<PathBuf>,
}

impl MyPlugin {
    fn append(&self, line: &str) -> PluginResult<()> {
        let path = self.log.get().ok_or("not loaded yet")?;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| PluginError::new(e.to_string()))?;
        writeln!(log, "{line}").map_err(|e| PluginError::new(e.to_string()))
    }
}

impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "my-plugin"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        // ctx.data_dir is the directory Echo hands you to keep files in.
        std::fs::create_dir_all(&ctx.data_dir).map_err(|e| PluginError::new(e.to_string()))?;
        let _ = self.log.set(ctx.data_dir.join("my-plugin.log"));
        self.append("my-plugin loaded")
    }

    fn on_unload(&self) -> PluginResult<()> {
        Ok(())
    }

    // The line that makes this an output plugin.
    fn as_output(&self) -> Option<&dyn OutputPlugin> {
        Some(self)
    }
}

impl OutputPlugin for MyPlugin {
    // Called after the text has been typed, on a thread of its own.
    fn on_transcript(&self, transcript: &Transcript) -> PluginResult<()> {
        let app = transcript.app.as_deref().unwrap_or("an unknown app");
        let chars = transcript.text.chars().count();
        self.append(&format!("delivered {chars} characters to {app}"))
    }
}

// Emits the symbols Echo looks up after opening the library, and catches a
// panic in any hook before it can leave it.
export_plugin!(MyPlugin);`;

const PLUGIN_JSON = `{
  "name": "my-plugin",
  "version": "0.1.0",
  "description": "What it does",
  "author": "You",
  "permissions": ["output"],
  "entry": "my_plugin.dll"
}`;

/**
 * The four capabilities, in the order a spoken sentence meets them. Each is
 * switched on by returning `Some(self)` from its `as_*` method on `Plugin`.
 */
const CAPABILITIES = [
  {
    method: "as_audio",
    trait: "AudioPlugin",
    when: "Every captured chunk, before speech detection. The hot path: its cost is logged after each recording, and a hook that fails stops being called until the plugin is enabled again.",
  },
  {
    method: "as_asr",
    trait: "AsrPlugin",
    when: "Each whole utterance, if the user picks your engine — it is listed as plugin:<name> and does nothing until chosen. A failure falls back to the offline engine.",
  },
  {
    method: "as_dictionary",
    trait: "DictionaryPlugin",
    when: "Asked when the plugin is enabled and whenever the dictionary changes. Entries apply after the user's own, so theirs win.",
  },
  {
    method: "as_output",
    trait: "OutputPlugin",
    when: "Each delivered transcript, after it has been typed, on a thread of its own. It observes: it cannot change the text or stop it arriving.",
  },
];

/** What `cargo build --release` really produces, per platform. */
const ARTIFACTS = [
  { os: "Windows", artifact: "my_plugin.dll" },
  { os: "macOS", artifact: "libmy_plugin.dylib" },
  { os: "Linux", artifact: "libmy_plugin.so" },
];

/** The folder-picker and name field behind "Scaffold a project". */
function Scaffold() {
  const [name, setName] = useState("my-plugin");
  const [created, setCreated] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Mirrors the backend's rule so the button can say no before a round trip.
  // The backend validates again regardless — this is a courtesy, not the guard.
  const legal = /^[a-z][a-z0-9-]*$/.test(name);

  async function run() {
    setError(null);
    setCreated(null);
    const parent = await open({ directory: true, multiple: false });
    if (typeof parent !== "string") return;

    setBusy(true);
    try {
      setCreated(await commands.scaffoldPlugin(parent, name));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-2.5">
      <div className="flex flex-wrap items-center gap-2">
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          spellCheck={false}
          className="field w-52 font-mono text-[13px]"
          aria-label="Plugin name"
          placeholder="my-plugin"
        />
        <button
          onClick={run}
          disabled={!legal || busy}
          className="btn-primary px-3 py-1.5 text-[13.5px]"
        >
          <FolderPlus className="h-3.5 w-3.5" />
          {busy ? "Writing…" : "Choose a folder…"}
        </button>
      </div>

      {!legal && (
        <p className="text-[13px] text-[var(--ink)]">
          A name is lowercase letters, digits and hyphens, starting with a
          letter — the same rule Cargo has for a package.
        </p>
      )}
      {error && <p className="text-[13px] font-medium text-[var(--ink)]">{error}</p>}
      {created && (
        <p className="text-[13px] text-[var(--ink-muted)]">
          Written to <span className="font-mono text-[var(--ink)]">{created}</span>.
          Step 4 builds it.
        </p>
      )}
    </div>
  );
}

export function BuildGuide() {
  return (
    <>
      <Group>
        <div className="glass flex items-start gap-2.5 rounded-lg px-3 py-2.5">
          <ShieldAlert className="mt-px h-4 w-4 shrink-0 text-[var(--ink)]" />
          <p className="max-w-[62ch] text-[13px] leading-snug text-[var(--ink-muted)]">
            <span className="font-medium text-[var(--ink)]">
              What you build here is not sandboxed.
            </span>{" "}
            A plugin runs inside Echo with your account's privileges — your
            files, your transcripts, your microphone, your stored API keys.
            That is worth knowing as an author too: anyone you hand a compiled
            plugin to is trusting you completely, and the{" "}
            <code className="font-mono">permissions</code> list you write is
            documentation, not a limit the app enforces.
          </p>
        </div>
      </Group>

      <Group
        title="What a plugin can do"
        hint="Only enabled plugins take part. Disabling one stops every call to it, mid-recording included."
      >
        <P>
          Echo opens your library, calls <code className="font-mono">on_load</code>{" "}
          when the plugin is enabled, and calls{" "}
          <code className="font-mono">on_unload</code> when it is disabled or Echo
          quits. To do more, implement a capability trait and return{" "}
          <code className="font-mono">Some(self)</code> from the matching method —
          without that line Echo never calls the trait, however it is written.
        </P>
        <dl className="max-w-[62ch] space-y-2.5 text-[13.5px] leading-relaxed">
          {CAPABILITIES.map(({ method, trait, when }) => (
            <div key={method}>
              <dt className="font-mono text-[12.5px] text-[var(--ink)]">
                {trait} · {method}
              </dt>
              <dd className="text-[var(--ink-muted)]">{when}</dd>
            </div>
          ))}
        </dl>
        <P>
          A panic in any hook is caught inside your library by{" "}
          <code className="font-mono">export_plugin!</code> and logged as an
          error, and nothing a plugin does can lose the user's text. Return a{" "}
          <code className="font-mono">PluginError</code> anyway: an error says
          what went wrong.
        </P>
      </Group>

      <Group title="Before you start">
        <P>
          You need a Rust toolchain — <code className="font-mono">rustup</code>{" "}
          from rust-lang.org. One constraint is worth reading twice: a plugin
          hands Echo a trait object across a dynamic library boundary, which is
          only sound if both sides agree on its shape. Echo checks the{" "}
          <code className="font-mono">echo-sdk</code> version your library was
          built against and refuses a mismatch, so a plugin built for an older
          Echo has to be rebuilt. What it cannot check is the compiler: build with
          the same Rust version and target as the Echo you are installing into,
          or it loads and then misbehaves.
        </P>
      </Group>

      <Group title="Five steps">
        <div className="space-y-7">
          <Step n={1} title="Start the project">
            <P>
              Echo can write a project that already compiles — the crate type,
              the manifest, the FFI entry point and a working{" "}
              <code className="font-mono">on_load</code>, filled in for this
              machine. Name it, choose a folder, and skip to step 4.
            </P>
            <Scaffold />
            <P>
              Rather do it by hand? <code className="font-mono">cargo new --lib my-plugin</code>,
              then steps 2 and 3 are the two files that differ from a normal
              crate.
            </P>
          </Step>

          <Step n={2} title="Make it a loadable library">
            <P>
              Two lines matter in{" "}
              <code className="font-mono">Cargo.toml</code>:{" "}
              <code className="font-mono">crate-type = ["cdylib"]</code>, because
              a normal Rust library cannot be opened at runtime, and the SDK,
              which is where the trait definition both sides share lives.
            </P>
            <Code>{CARGO_TOML}</Code>
          </Step>

          <Step n={3} title="Implement Plugin, and export it">
            <P>
              Four lifecycle methods, one capability and a macro. This is the
              scaffold's plugin: an output plugin that notes how long each
              delivered transcript was and which app it went to — not the words
              themselves.
            </P>
            <Code>{LIB_RS}</Code>
            <P>
              <code className="font-mono">export_plugin!</code> emits{" "}
              <code className="font-mono">echo_plugin_create</code> and the SDK
              version check Echo reads first, and wraps your type so a panic
              stays inside the library. Your type must implement{" "}
              <code className="font-mono">Default</code>, and this is the
              boilerplate you should never hand-write — it is the only{" "}
              <code className="font-mono">unsafe</code> in the contract.
            </P>
          </Step>

          <Step n={4} title="Describe it, and build">
            <P>
              A <code className="font-mono">plugin.json</code> sits next to the
              built library. <code className="font-mono">permissions</code> may
              list <code className="font-mono">asr</code>,{" "}
              <code className="font-mono">output</code>,{" "}
              <code className="font-mono">audio</code> or{" "}
              <code className="font-mono">dictionary</code>, and is advisory.
            </P>
            <Code>{PLUGIN_JSON}</Code>
            <Code>cargo build --release</Code>
            <P>
              <code className="font-mono">entry</code> is the file name Echo
              stores and loads your library under, and it is the one field people
              get wrong: Cargo turns hyphens into underscores and decorates the
              name differently on each platform. A scaffolded project already has
              this right.
            </P>
            <div className="overflow-x-auto">
              <table className="w-full text-left text-[13px]">
                <thead>
                  <tr className="border-b border-[var(--hairline)] text-[var(--ink-muted)]">
                    <th className="py-1.5 pr-6 font-medium">Platform</th>
                    <th className="py-1.5 font-medium">
                      target/release/… and your <code className="font-mono">entry</code>
                    </th>
                  </tr>
                </thead>
                <tbody className="text-[var(--ink-muted)]">
                  {ARTIFACTS.map(({ os, artifact }) => (
                    <tr key={os} className="border-b border-[var(--hairline)] last:border-0">
                      <td className="py-1.5 pr-6">{os}</td>
                      <td className="py-1.5 font-mono text-[var(--ink)]">{artifact}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Step>

          <Step n={5} title="Install it into Echo">
            <P>
              Copy <code className="font-mono">plugin.json</code> next to the
              built library — Echo reads the manifest from the directory of the
              file you pick, so the pair have to travel together. Then{" "}
              <span className="text-[var(--ink)]">Installed</span> →{" "}
              <span className="text-[var(--ink)]">Install from file</span>, choose
              the library, and confirm the warning. Echo copies both into its own
              plugins directory, records a SHA-256 of the copy, and loads it.
            </P>
            <P>
              Changing your plugin later means building again and installing
              again. The fingerprint is checked on every load, so a library
              edited underneath Echo is disabled rather than loaded — including
              when the edit was your own rebuild.
            </P>
          </Step>
        </div>
      </Group>
    </>
  );
}
