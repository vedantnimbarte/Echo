/**
 * Translation, without a framework.
 *
 * Echo's strings are few and its rendering is synchronous, so the usual i18n
 * libraries would be several hundred kilobytes to solve problems this app does
 * not have — no lazy namespaces, no plural rule engine, no backend loader.
 * What is actually needed is a lookup with a fallback and a way to interpolate
 * a value, which is what this is.
 *
 * Catalogues are plain JSON keyed by dotted string ids. English is the source
 * of truth: a key missing from a translation falls back to English rather than
 * rendering blank, and a key missing from English renders its own id, which
 * makes an untranslated string obvious instead of invisible.
 *
 * Adding a language means adding `locales/<code>.json` and listing it in
 * {@link LOCALES}. Only the keys present are used; partial catalogues are fine.
 */

import en from "./locales/en.json";
import es from "./locales/es.json";
import de from "./locales/de.json";
import fr from "./locales/fr.json";

type Catalogue = Record<string, string>;

/** Every catalogue Echo ships, by BCP-47 primary subtag. */
export const LOCALES: Record<string, { label: string; catalogue: Catalogue }> = {
  en: { label: "English", catalogue: en },
  es: { label: "Español", catalogue: es },
  de: { label: "Deutsch", catalogue: de },
  fr: { label: "Français", catalogue: fr },
};

export const DEFAULT_LOCALE = "en";

let current = DEFAULT_LOCALE;

/**
 * Resolve a stored preference (or "auto") to a locale we actually have.
 *
 * Matching is on the primary subtag, so "pt-BR" finds "pt" — a regional
 * variant is far closer to its base language than to English.
 */
export function resolveLocale(preference: string | null | undefined): string {
  const wanted =
    !preference || preference === "auto"
      ? typeof navigator !== "undefined"
        ? navigator.language
        : DEFAULT_LOCALE
      : preference;

  const primary = (wanted ?? DEFAULT_LOCALE).split("-")[0].toLowerCase();
  return primary in LOCALES ? primary : DEFAULT_LOCALE;
}

/** Set the active locale. Returns what was actually selected. */
export function setLocale(preference: string | null | undefined): string {
  current = resolveLocale(preference);
  return current;
}

export function getLocale(): string {
  return current;
}

/**
 * Translate `key`, interpolating `{name}` placeholders from `vars`.
 *
 * Falls back through the active catalogue, then English, then the key itself.
 */
export function t(key: string, vars?: Record<string, string | number>): string {
  const template =
    LOCALES[current]?.catalogue[key] ?? LOCALES[DEFAULT_LOCALE].catalogue[key] ?? key;

  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in vars ? String(vars[name]) : whole,
  );
}
