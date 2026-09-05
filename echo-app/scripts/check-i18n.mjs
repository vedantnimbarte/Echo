#!/usr/bin/env node
/**
 * Verify every translation catalogue against English.
 *
 * A missing key is not a crash — `t()` falls back to English — which is exactly
 * why it needs checking: an untranslated string looks fine to whoever is not
 * reading that language, and nothing ever surfaces it. A dropped placeholder is
 * worse and just as quiet: `{name}` missing from a translation renders a
 * sentence with a hole in it.
 *
 * Plain Node, no dependencies, so it runs in CI without an install step.
 *
 *   node scripts/check-i18n.mjs
 */

import { readFileSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const LOCALES_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", "src", "i18n", "locales");
const SOURCE = "en";

const read = (locale) =>
  JSON.parse(readFileSync(join(LOCALES_DIR, `${locale}.json`), "utf8"));

/** `{name}` placeholders a template expects, as a sorted list. */
const placeholders = (value) =>
  [...value.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

const english = read(SOURCE);
const locales = readdirSync(LOCALES_DIR)
  .filter((f) => f.endsWith(".json"))
  .map((f) => f.replace(/\.json$/, ""))
  .filter((l) => l !== SOURCE);

let failed = false;

for (const locale of locales) {
  const catalogue = read(locale);
  const problems = [];

  for (const key of Object.keys(english)) {
    if (!(key in catalogue)) {
      problems.push(`missing:  ${key}`);
      continue;
    }
    const expected = placeholders(english[key]);
    const actual = placeholders(catalogue[key]);
    if (expected.join() !== actual.join()) {
      problems.push(
        `placeholders differ in ${key}: expected {${expected}}, got {${actual}}`,
      );
    }
  }

  // An extra key is dead weight, and usually a rename that only landed in one
  // file — worth reporting, but not worth failing a build over.
  for (const key of Object.keys(catalogue)) {
    if (!(key in english)) problems.push(`unused:   ${key}`);
  }

  const fatal = problems.filter((p) => !p.startsWith("unused:"));
  if (problems.length === 0) {
    console.log(`${locale}: ok (${Object.keys(english).length} keys)`);
  } else {
    console.log(`${locale}:`);
    for (const p of problems) console.log(`  ${p}`);
    if (fatal.length > 0) failed = true;
  }
}

if (failed) {
  console.error("\ni18n check failed.");
  process.exit(1);
}
console.log("\nAll catalogues complete.");
