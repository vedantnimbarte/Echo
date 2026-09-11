import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type CloudProvider, type ProviderField } from "../../ipc/commands";
import { Field } from "../common/Page";

/**
 * Cloud providers: the list you choose your engine from, and the keys that let
 * you.
 *
 * This used to be two controls in two places — a dropdown at the top of the
 * page offering only providers that already had keys, and a stack of key forms
 * far below it. Every provider showed four fields at once, so the common case
 * (one provider, one key) arrived as forty inputs, and the control that picked
 * a provider was nowhere near the one that made it pickable.
 *
 * Now it is one list. A line each, and the row you open shows its fields.
 *
 * Opening a row is not choosing it: you often open one to change its model, and
 * having that redirect your dictation would be a trap. Choosing is the button
 * that says so.
 */

/** Result of the last "Test" press, so the row can say what happened. */
type TestState = { ok: boolean; message: string } | null;

/** What the right-hand end of a collapsed row says about that provider. */
function status(p: CloudProvider, isActive: boolean): string {
  if (isActive) return "In use";
  // `available` is false when the provider's own config is incomplete, which
  // for every catalog row means one of these two is still empty.
  if (!p.available) return p.needs_region && !p.region ? "Region needed" : "Endpoint needed";
  return p.key_set ? "Key stored" : "No key";
}

function ProviderBody({
  provider,
  isActive,
  onUse,
}: {
  provider: CloudProvider;
  isActive: boolean;
  onUse: () => void;
}) {
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
    <div className="space-y-5 border-t border-[var(--hairline)] px-4 py-5">
      <p className="max-w-[52ch] text-[13.5px] leading-relaxed text-[var(--ink-muted)]">
        {provider.note}
      </p>

      <div className="space-y-2">
        <span className="block text-[13.5px] font-medium text-[var(--ink-muted)]">
          API key
        </span>
        <div className="flex gap-2">
          <input
            type="password"
            placeholder={provider.key_set ? "Stored — type a new one to replace it" : "Paste your key"}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            className="field flex-1 rounded-lg"
            aria-label={`${provider.label} API key`}
          />
          <button
            onClick={save}
            disabled={!value}
            className="btn-primary shrink-0 rounded-lg px-3.5 text-[13.5px]"
          >
            Save key
          </button>
        </div>
        {provider.key_set && (
          <div className="flex gap-2 pt-0.5">
            <button
              onClick={() => runTest.mutate()}
              disabled={runTest.isPending || !provider.available}
              className="btn-ghost rounded-lg px-3 py-1.5 text-[13px] text-[var(--ink-muted)] hover:text-[var(--ink)]"
            >
              {runTest.isPending ? "Testing…" : "Test the key"}
            </button>
            <button
              onClick={remove}
              className="btn-ghost rounded-lg px-3 py-1.5 text-[13px] text-[var(--ink-muted)] hover:text-[var(--ink)]"
            >
              Remove
            </button>
          </div>
        )}
        {test && (
          <p
            className={
              test.ok
                ? "text-[13px] text-[var(--ink)]"
                : "text-[13px] text-[var(--danger,#e5484d)]"
            }
          >
            {test.message}
          </p>
        )}
      </div>

      {/* An endpoint or region field only appears for providers that need one,
          so the common case stays a key and a model. */}
      {(provider.needs_endpoint || provider.endpoint !== provider.default_endpoint) && (
        <Field label="Endpoint">
          <input
            type="text"
            defaultValue={provider.endpoint}
            placeholder={provider.default_endpoint || "https://your-endpoint/v1"}
            onBlur={(e) => saveField.mutate({ field: "endpoint", v: e.target.value })}
            className="field w-full rounded-lg"
            aria-label={`${provider.label} endpoint`}
          />
        </Field>
      )}

      {provider.needs_region && (
        <Field label="Region">
          <input
            type="text"
            defaultValue={provider.region ?? ""}
            placeholder="westeurope"
            onBlur={(e) => saveField.mutate({ field: "region", v: e.target.value })}
            className="field w-full rounded-lg"
            aria-label={`${provider.label} region`}
          />
        </Field>
      )}

      {/* Free text with suggestions rather than a closed dropdown: a provider
          ships new model names faster than Echo ships releases, and a
          self-hosted endpoint's model name is whatever the user called it. */}
      <Field label="Model">
        <input
          type="text"
          list={modelListId}
          defaultValue={provider.model}
          onBlur={(e) => saveField.mutate({ field: "model", v: e.target.value })}
          className="field w-full rounded-lg"
          aria-label={`${provider.label} model`}
        />
      </Field>
      <datalist id={modelListId}>
        {provider.models.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>

      <div className="flex items-center justify-between gap-4 pt-0.5">
        <a
          href={provider.docs_url}
          target="_blank"
          rel="noreferrer"
          className="text-[13px] text-[var(--ink-muted)] underline-offset-2 hover:text-[var(--ink)] hover:underline"
        >
          Where to get a key
        </a>
        {isActive ? (
          <span className="text-[13px] text-[var(--ink-muted)]">
            Dictation is running on {provider.label}.
          </span>
        ) : (
          <button
            onClick={onUse}
            disabled={!provider.key_set || !provider.available}
            className="btn-primary rounded-lg px-3.5 py-1.5 text-[13.5px]"
          >
            Dictate with {provider.label}
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * Pick a cloud provider, and hold the keys they need. Keys live in the OS
 * keychain, never in echo.db.
 */
export function CloudProviders() {
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: ["cloud-providers"],
    queryFn: commands.listCloudProviders,
  });
  // `?? []` rather than a destructuring default: that only fires on undefined,
  // and a backend that answers null would take the list down with it.
  const providers = data ?? [];
  // Read here rather than passed in: the same query is already cached by the
  // settings page, and a prop would only be a second way to be wrong.
  const { data: active } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  // null = untouched, so the active row opens itself; "" = closed on purpose.
  const [opened, setOpened] = useState<string | null>(null);

  const use = useMutation({
    mutationFn: (id: string) => commands.setAsrProvider(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "asr_provider"] }),
  });

  const activeCloud = providers.find((p) => p.id === active) ?? null;
  // Nothing chosen yet: the row already in use opens itself, so the list lands
  // on the thing you came to change.
  const openId = opened ?? activeCloud?.id ?? null;

  return (
    <div className="space-y-4">
      {!activeCloud && (
        <p className="max-w-[56ch] text-[13.5px] leading-relaxed text-[var(--ink-muted)]">
          Dictation is still running on this machine. Add a key below, then
          choose that provider to send audio to it instead.
        </p>
      )}

      <div className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-[var(--surface-1)]">
        {providers.map((p, i) => {
          const isActive = p.id === active;
          const isOpen = p.id === openId;
          return (
            <div
              key={p.id}
              // The lift belongs to the whole open row, header and fields
              // together — on the header alone it reads as a selected line with
              // a form loose underneath it.
              className={
                (i > 0 ? "border-t border-[var(--hairline)] " : "") +
                (isOpen ? "bg-[var(--surface-2)]" : "")
              }
            >
              <button
                onClick={() => setOpened(isOpen ? "" : p.id)}
                aria-expanded={isOpen}
                className="flex w-full items-center gap-3 px-4 py-3 text-left transition hover:bg-[var(--surface-2)]"
              >
                <span
                  aria-hidden
                  className={
                    "h-1.5 w-1.5 shrink-0 rounded-full " +
                    (isActive ? "bg-[var(--ink)]" : "bg-[var(--hairline-strong)]")
                  }
                />
                <span className="flex-1 truncate text-[14.5px] font-medium text-[var(--ink)]">
                  {p.label}
                </span>
                <span
                  className={
                    "shrink-0 text-[13px] " +
                    (isActive ? "text-[var(--ink)]" : "text-[var(--ink-faint)]")
                  }
                >
                  {status(p, isActive)}
                </span>
              </button>
              {isOpen && (
                <ProviderBody
                  provider={p}
                  isActive={isActive}
                  onUse={() => use.mutate(p.id)}
                />
              )}
            </div>
          );
        })}
      </div>

      {use.isError && (
        <p className="text-[13px] text-[var(--danger,#e5484d)]">{String(use.error)}</p>
      )}
    </div>
  );
}
