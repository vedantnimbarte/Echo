import { useQueries, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";

const fieldCls =
  "w-full rounded-lg border border-white/10 bg-white/5 px-2.5 py-1.5 text-[13px] text-[var(--ink)] outline-none transition focus:border-[var(--aurora-2)]/60 focus:bg-white/8";

/** Settings keys this panel owns, with the defaults the backend also uses. */
const DEFAULTS = {
  command_mode_enabled: "false",
  command_prefix: "command",
  command_llm_provider: "ollama",
  command_llm_model: "llama3.2",
  ollama_endpoint: "http://localhost:11434",
} as const;

type Key = keyof typeof DEFAULTS;

/**
 * Command mode: say the prefix word and the rest of the sentence becomes an
 * instruction for an LLM rather than text to type. With a selection in the
 * focused app the instruction is applied to it; otherwise the answer is
 * inserted at the cursor.
 */
export function CommandMode() {
  const qc = useQueryClient();

  const queries = useQueries({
    queries: (Object.keys(DEFAULTS) as Key[]).map((key) => ({
      queryKey: ["setting", key],
      queryFn: () => commands.getSetting(key),
    })),
  });
  const [enabledRaw, prefix, provider, model, endpoint] = (
    Object.keys(DEFAULTS) as Key[]
  ).map((key, i) => queries[i].data ?? DEFAULTS[key]);
  const enabled = enabledRaw === "true";

  async function save(key: Key, value: string) {
    await commands.setSetting(key, value);
    qc.invalidateQueries({ queryKey: ["setting", key] });
  }

  return (
    <div className="space-y-3">
      <label className="flex items-start gap-2.5">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) =>
            save("command_mode_enabled", e.target.checked ? "true" : "false")
          }
          className="mt-0.5 h-3.5 w-3.5 accent-[var(--aurora-2)]"
        />
        <span className="text-[12px] leading-snug">
          Treat “{prefix} …” as an instruction
          <span className="block text-[10.5px] text-[var(--ink-muted)]">
            Select some text and say “{prefix}, make this more formal” to rewrite
            it. With nothing selected, the answer is typed at your cursor.
          </span>
        </span>
      </label>

      <label className="block space-y-1">
        <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
          Trigger word
        </span>
        <input
          className={fieldCls}
          defaultValue={prefix}
          onBlur={(e) => {
            const next = e.target.value.trim();
            if (next && next !== prefix) save("command_prefix", next);
          }}
        />
        <span className="text-[10.5px] leading-snug text-[var(--ink-muted)]">
          Pick a word you rarely dictate. Anything not starting with it is typed
          as normal text.
        </span>
      </label>

      <label className="block space-y-1">
        <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
          Model runs on
        </span>
        <select
          className={fieldCls}
          value={provider}
          onChange={(e) => save("command_llm_provider", e.target.value)}
        >
          <option value="ollama">Ollama — on this machine</option>
          <option value="openai">OpenAI — uses your stored API key</option>
        </select>
        {provider === "openai" && (
          <span className="block text-[10.5px] leading-snug text-amber-400/90">
            Selected text is sent to OpenAI. Ollama keeps it on your machine.
          </span>
        )}
      </label>

      <label className="block space-y-1">
        <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
          Model
        </span>
        <input
          className={fieldCls}
          defaultValue={model}
          placeholder={provider === "openai" ? "gpt-4o-mini" : "llama3.2"}
          onBlur={(e) => {
            const next = e.target.value.trim();
            if (next && next !== model) save("command_llm_model", next);
          }}
        />
      </label>

      {provider === "ollama" && (
        <label className="block space-y-1">
          <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
            Ollama address
          </span>
          <input
            className={fieldCls}
            defaultValue={endpoint}
            onBlur={(e) => {
              const next = e.target.value.trim();
              if (next && next !== endpoint) save("ollama_endpoint", next);
            }}
          />
          <span className="text-[10.5px] leading-snug text-[var(--ink-muted)]">
            Needs Ollama running locally (<code>ollama serve</code>) with the model
            pulled.
          </span>
        </label>
      )}
    </div>
  );
}
