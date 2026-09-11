import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type CloudProvider, type ProviderField } from "../../ipc/commands";

/** Result of the last "Test" press, so the row can say what happened. */
type TestState = { ok: boolean; message: string } | null;

function ProviderRow({ provider }: { provider: CloudProvider }) {
  const qc = useQueryClient();
  const [value, setValue] = useState("");
  const [test, setTest] = useState<TestState>(null);

  const refresh = () => qc.invalidateQueries({ queryKey: ["cloud-providers"] });

  const saveField = useMutation({
    mutationFn: ({ field, v }: { field: ProviderField; v: string }) =>
      commands.setProviderSetting(provider.id, field, v),
    onSuccess: refresh,
  });

  // A key test is a real network round trip, so its outcome is held rather than
  // flashed: the whole point is that someone can read why it failed.
  const runTest = useMutation({
    mutationFn: () => commands.testApiKey(provider.id),
    onSuccess: () => setTest({ ok: true, message: "Key works" }),
    onError: (e) => setTest({ ok: false, message: String(e) }),
  });

  async function save() {
    if (!value) return;
    await commands.setApiKey(provider.id, value);
    setValue("");
    setTest(null);
    refresh();
  }

  async function remove() {
    await commands.removeApiKey(provider.id);
    setTest(null);
    refresh();
  }

  const modelListId = `models-${provider.id}`;

  return (
    <div className="space-y-2 rounded-lg glass px-3 py-2">
      <div className="flex items-center justify-between">
        <span className="text-[12px] font-medium text-[var(--ink)]">
          {provider.label}
          {!provider.available && (
            <span className="ml-1.5 text-[10.5px] text-[var(--ink-faint)]">
              coming soon
            </span>
          )}
        </span>
        <span
          className={
            provider.key_set
              ? "text-[11px] text-[var(--ink)]"
              : "text-[11px] text-[var(--ink-faint)]"
          }
        >
          {provider.key_set ? "Key stored" : "No key"}
        </span>
      </div>

      <p className="text-[11px] leading-snug text-[var(--ink-muted)]">{provider.note}</p>

      <div className="flex gap-1.5">
        <input
          type="password"
          placeholder={provider.key_set ? "••••••••  (enter to replace)" : "API key"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          className="field flex-1 rounded-md text-[12px]"
          aria-label={`${provider.label} API key`}
        />
        <button
          onClick={save}
          disabled={!value}
          className="btn-primary rounded-md px-2.5 py-1.5 text-[11px]"
        >
          Save
        </button>
        {provider.key_set && (
          <>
            <button
              onClick={() => runTest.mutate()}
              disabled={runTest.isPending || !provider.available}
              className="btn-ghost rounded-md px-2.5 py-1.5 text-[11px] text-[var(--ink-muted)] hover:text-[var(--ink)]"
            >
              {runTest.isPending ? "Testing…" : "Test"}
            </button>
            <button
              onClick={remove}
              className="btn-ghost rounded-md px-2.5 py-1.5 text-[11px] text-[var(--ink-muted)] hover:text-[var(--ink)]"
            >
              Remove
            </button>
          </>
        )}
      </div>

      {/* An endpoint or region field only appears for providers that need one,
          so the common case stays a single key box. */}
      {(provider.needs_endpoint || provider.endpoint !== provider.default_endpoint) && (
        <input
          type="text"
          defaultValue={provider.endpoint}
          placeholder={provider.default_endpoint || "https://your-endpoint/v1"}
          onBlur={(e) => saveField.mutate({ field: "endpoint", v: e.target.value })}
          className="field w-full rounded-md text-[12px]"
          aria-label={`${provider.label} endpoint`}
        />
      )}

      {provider.needs_region && (
        <input
          type="text"
          defaultValue={provider.region ?? ""}
          placeholder="Region, e.g. westeurope"
          onBlur={(e) => saveField.mutate({ field: "region", v: e.target.value })}
          className="field w-full rounded-md text-[12px]"
          aria-label={`${provider.label} region`}
        />
      )}

      {/* Free text with suggestions rather than a closed dropdown: a provider
          ships new model names faster than Echo ships releases, and a
          self-hosted endpoint's model name is whatever the user called it. */}
      <input
        type="text"
        list={modelListId}
        defaultValue={provider.model}
        placeholder="Model"
        onBlur={(e) => saveField.mutate({ field: "model", v: e.target.value })}
        className="field w-full rounded-md text-[12px]"
        aria-label={`${provider.label} model`}
      />
      <datalist id={modelListId}>
        {provider.models.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>

      {test && (
        <p
          className={
            test.ok
              ? "text-[11px] text-[var(--ink)]"
              : "text-[11px] text-[var(--danger,#e5484d)]"
          }
        >
          {test.message}
        </p>
      )}

      <a
        href={provider.docs_url}
        target="_blank"
        rel="noreferrer"
        className="text-[11px] text-[var(--ink-muted)] underline-offset-2 hover:text-[var(--ink)] hover:underline"
      >
        Get an API key →
      </a>
    </div>
  );
}

/** Manage API keys and settings for cloud ASR providers. Keys live in the OS keychain. */
export function CloudProviders() {
  const { data: providers } = useQuery({
    queryKey: ["cloud-providers"],
    queryFn: commands.listCloudProviders,
  });

  return (
    <div className="space-y-1.5">
      <span className="text-[11.5px] font-medium text-[var(--ink-muted)]">
        Cloud provider API keys
      </span>
      <div className="space-y-1.5">
        {(providers ?? []).map((p) => (
          <ProviderRow key={p.id} provider={p} />
        ))}
      </div>
    </div>
  );
}
