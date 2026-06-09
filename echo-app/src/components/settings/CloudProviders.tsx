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
    <div className="rounded-lg border border-zinc-700 bg-zinc-800/50 px-3 py-2 space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-sm text-zinc-200">{meta.label}</span>
        <span className={isSet ? "text-xs text-green-400" : "text-xs text-zinc-500"}>
          {isSet ? "Key stored" : "No key"}
        </span>
      </div>
      <div className="flex gap-2">
        <input
          type="password"
          placeholder={isSet ? "••••••••  (enter to replace)" : "API key"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          className="flex-1 bg-zinc-900 border border-zinc-700 rounded-md px-3 py-1.5 text-sm text-zinc-100 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-violet-500"
        />
        <button
          onClick={save}
          disabled={!value}
          className="rounded-md bg-violet-600 hover:bg-violet-700 disabled:opacity-50 px-3 py-1.5 text-xs text-white"
        >
          Save
        </button>
        {isSet && (
          <button
            onClick={remove}
            className="rounded-md border border-zinc-600 px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-700"
          >
            Remove
          </button>
        )}
      </div>
      <a
        href={meta.docs}
        target="_blank"
        rel="noreferrer"
        className="text-xs text-violet-400 hover:underline"
      >
        Get an API key →
      </a>
    </div>
  );
}

/** Manage API keys for cloud ASR providers. Keys live in the OS keychain. */
export function CloudProviders() {
  return (
    <div className="space-y-2">
      <span className="text-sm text-zinc-400">Cloud provider API keys</span>
      <div className="space-y-2">
        {PROVIDERS.map((p) => (
          <ProviderRow key={p.id} meta={p} />
        ))}
      </div>
    </div>
  );
}
