import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";

interface ProviderMeta {
  id: string;
  label: string;
  docs: string;
}

const PROVIDERS: ProviderMeta[] = [
  { id: "openai", label: "OpenAI Whisper", docs: "https://platform.openai.com/api-keys" },
  { id: "groq", label: "Groq", docs: "https://console.groq.com/keys" },
  { id: "deepgram", label: "Deepgram", docs: "https://console.deepgram.com/" },
];

function ProviderRow({ meta }: { meta: ProviderMeta }) {
  const qc = useQueryClient();
  const [value, setValue] = useState("");

  const { data: isSet } = useQuery({
    queryKey: ["api-key-set", meta.id],
    queryFn: () => commands.getApiKeySet(meta.id),
  });

  async function save() {
    if (!value) return;
    await commands.setApiKey(meta.id, value);
    setValue("");
    qc.invalidateQueries({ queryKey: ["api-key-set", meta.id] });
  }

  async function remove() {
    await commands.removeApiKey(meta.id);
    qc.invalidateQueries({ queryKey: ["api-key-set", meta.id] });
  }

  return (
    <div className="space-y-2 rounded-lg border border-white/8 bg-white/[0.025] px-3 py-2">
      <div className="flex items-center justify-between">
        <span className="text-[12px] font-medium text-[var(--ink)]">{meta.label}</span>
        <span
          className={
            isSet ? "text-[11px] text-emerald-400" : "text-[11px] text-[var(--ink-faint)]"
          }
        >
          {isSet ? "Key stored" : "No key"}
        </span>
      </div>
      <div className="flex gap-1.5">
        <input
          type="password"
          placeholder={isSet ? "••••••••  (enter to replace)" : "API key"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          className="flex-1 rounded-md border border-white/10 bg-white/5 px-2.5 py-1.5 text-[12px] text-[var(--ink)] outline-none transition placeholder:text-[var(--ink-faint)] focus:border-[var(--aurora-2)]/60 focus:bg-white/8"
        />
        <button
          onClick={save}
          disabled={!value}
          className="rounded-md bg-[var(--aurora-2)] px-2.5 py-1.5 text-[11px] font-medium text-white transition hover:brightness-110 disabled:opacity-50"
        >
          Save
        </button>
        {isSet && (
          <button
            onClick={remove}
            className="rounded-md border border-white/12 px-2.5 py-1.5 text-[11px] text-[var(--ink-muted)] transition hover:bg-white/8 hover:text-[var(--ink)]"
          >
            Remove
          </button>
        )}
      </div>
      <a
        href={meta.docs}
        target="_blank"
        rel="noreferrer"
        className="text-[11px] text-[var(--aurora-1)] hover:underline"
      >
        Get an API key →
      </a>
    </div>
  );
}

/** Manage API keys for cloud ASR providers. Keys live in the OS keychain. */
export function CloudProviders() {
  return (
    <div className="space-y-1.5">
      <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
        Cloud provider API keys
      </span>
      <div className="space-y-1.5">
        {PROVIDERS.map((p) => (
          <ProviderRow key={p.id} meta={p} />
        ))}
      </div>
    </div>
  );
}
