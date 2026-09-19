#!/usr/bin/env node
/**
 * SOURCE OF TRUTH KEYWORDS: sot, sotSearch, SotHeader, parseSotHeaders,
 *   collectSourceFiles, SOT_MARKER, matchKeyword, renderFileList, renderHeaders
 * WHAT:  The SOURCE OF TRUTH keyword search. Given a keyword, prints the files
 *        whose SOT header claims to own that symbol — `npm run sot <keyword>` —
 *        or those headers in full with `--show`.
 * WHY:   The header convention only pays off if something reads it. This is the
 *        `grep -l` step of the navigation loop in docs/06: it answers "where
 *        could this live" in one screen instead of forty, which is what keeps
 *        context cost flat as the codebase grows.
 *
 *        Read-only by design. It changes nothing, takes no arguments that could
 *        name a file to write, and depends on nothing outside Node's standard
 *        library — so running it is never a decision anyone has to think about.
 * WHERE: Invoked via the `sot` / `sot:show` package scripts. Reads src/ and
 *        src-tauri/src/.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative, extname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { dirname } from 'node:path'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')

/** Directories that hold authored source. Generated output is excluded below. */
const SEARCH_ROOTS = ['src', 'src-tauri/src', 'scripts']
const SOURCE_EXTENSIONS = new Set(['.rs', '.ts', '.tsx', '.mjs', '.js', '.css', '.sql'])
const IGNORED_DIRS = new Set(['node_modules', 'target', 'dist', '.git', 'gen'])

/**
 * Generated files carry headers too, but they are never the answer to "where
 * should I put this" — pointing an agent at a generated file invites an edit
 * that the next build silently reverts.
 */
const GENERATED = [/bindings\.ts$/, /registry\.generated\.ts$/]

const SOT_MARKER = 'SOURCE OF TRUTH KEYWORDS:'
/** A header section ends where the next labelled section begins. */
const SECTION_LABEL = /^\s*(?:\*|\/\/|--)?\s*(WHAT|WHY|WHERE)\s*:/i
/**
 * Strips the comment furniture from a captured line. Covers Rust's inner and
 * outer doc forms (`/*!`, `/**`, `//!`, `///`) as well as plain `*`, `//` and
 * SQL's `--`, because the header has to read identically in every language the
 * repository uses.
 */
const COMMENT_PREFIX = /^\s*(?:\*\/|\*|\/\*[*!]?|\/\/[!/]?|--)?\s?/

/**
 * WHAT:  Walks the search roots and yields every authored source file path.
 * WHY:   Recursion with an explicit ignore set beats a glob dependency — this
 *        script must run with zero installs so it works on a fresh clone.
 */
function collectSourceFiles(dir, out = []) {
  let entries
  try {
    entries = readdirSync(dir)
  } catch {
    return out
  }
  for (const entry of entries) {
    if (IGNORED_DIRS.has(entry)) continue
    const full = join(dir, entry)
    let stats
    try {
      stats = statSync(full)
    } catch {
      continue
    }
    if (stats.isDirectory()) {
      collectSourceFiles(full, out)
    } else if (SOURCE_EXTENSIONS.has(extname(entry))) {
      const rel = relative(ROOT, full)
      if (!GENERATED.some((pattern) => pattern.test(rel))) out.push(full)
    }
  }
  return out
}

/**
 * WHAT:  Extracts every SOT header in one file as { keywords, lines, line }.
 * WHY:   Keyword lists wrap across lines, so a single-line grep misses roughly
 *        half of them. Capturing until the next WHAT/WHY/WHERE label is what
 *        makes a wrapped keyword findable.
 */
function parseSotHeaders(filePath) {
  const text = readFileSync(filePath, 'utf8')
  const lines = text.split('\n')
  const headers = []

  for (let i = 0; i < lines.length; i++) {
    const markerAt = lines[i].indexOf(SOT_MARKER)
    if (markerAt === -1) continue

    const keywordParts = [lines[i].slice(markerAt + SOT_MARKER.length)]
    const block = [lines[i]]

    // Keywords continue until the next labelled section or the comment ends.
    let j = i + 1
    for (; j < lines.length; j++) {
      const line = lines[j]
      if (SECTION_LABEL.test(line) || line.includes('*/') || line.trim() === '') break
      keywordParts.push(line.replace(COMMENT_PREFIX, ''))
      block.push(line)
    }
    // Keep the WHAT/WHY/WHERE body for --show.
    for (; j < lines.length; j++) {
      const line = lines[j]
      block.push(line)
      if (line.includes('*/')) break
      if (block.length > 40) break
    }

    const keywords = keywordParts
      .join(' ')
      .split(',')
      .map((k) => k.replace(COMMENT_PREFIX, '').trim())
      .filter(Boolean)

    headers.push({ keywords, block, line: i + 1 })
    i = j
  }
  return headers
}

/** Case-insensitive substring match, so `sot session` finds `SessionMachine`. */
function matchKeyword(keywords, query) {
  const needle = query.toLowerCase()
  return keywords.some((k) => k.toLowerCase().includes(needle))
}

/**
 * SOURCE OF TRUTH KEYWORDS: reportMissing, coverage, --missing
 * WHAT:  Lists the source files that carry no SOT header, with a count.
 * WHY:   THE GAP HAS TO BE VISIBLE OR IT IS NOT A GAP, it is just the status
 *        quo. The convention arrived with this tool and most of the tree
 *        predates it, so the honest position is a number that can be watched
 *        going down rather than a claim that the codebase is indexed.
 *
 *        It deliberately does not fail. A check that broke the build over
 *        undocumented files would be satisfied by a wave of headers written to
 *        silence it, which is worse than none: a header that restates the
 *        filename costs a reader the time it takes to discover it says nothing.
 * WHERE: `npm run sot:missing`.
 */
function reportMissing(files) {
  const missing = files.filter((file) => parseSotHeaders(file).length === 0)
  for (const file of missing) console.log(relative(ROOT, file))
  const covered = files.length - missing.length
  const percent = Math.round((covered / files.length) * 100)
  console.log(`
${covered}/${files.length} files carry a SOT header (${percent}%).`)
  console.log('Add one when you next work in a file that has none — not in bulk.')
}

function main() {
  const argv = process.argv.slice(2)
  const show = argv.includes('--show')
  const missing = argv.includes('--missing')
  const query = argv.filter((a) => a !== '--show' && a !== '--missing').join(' ').trim()

  if (missing) {
    reportMissing(SEARCH_ROOTS.flatMap((root) => collectSourceFiles(join(ROOT, root))))
    return
  }

  if (!query) {
    console.error('Usage: npm run sot <keyword>        # files that own the symbol')
    console.error('       npm run sot:show <keyword>   # the same, with headers')
    console.error('       npm run sot:missing          # files with no header yet')
    process.exit(1)
  }

  const files = SEARCH_ROOTS.flatMap((root) => collectSourceFiles(join(ROOT, root)))
  const hits = []

  for (const file of files) {
    const matched = parseSotHeaders(file).filter((h) => matchKeyword(h.keywords, query))
    if (matched.length) hits.push({ file: relative(ROOT, file), headers: matched })
  }

  if (!hits.length) {
    console.log(`No SOURCE OF TRUTH header claims "${query}".`)
    console.log('Search the codebase normally before creating it — and give the')
    console.log('new thing its own SOT header so the next agent finds it.')
    process.exit(0)
  }

  if (!show) {
    for (const hit of hits) console.log(hit.file)
    console.log(`\n${hits.length} file(s). Read the header before opening the file.`)
    return
  }

  for (const hit of hits) {
    console.log(`\n\x1b[1m${hit.file}\x1b[0m`)
    for (const header of hit.headers) {
      console.log(`  \x1b[2m:${header.line}\x1b[0m`)
      for (const line of header.block) console.log(`  ${line}`)
    }
  }
  console.log(`\n${hits.length} file(s).`)
}

main()
