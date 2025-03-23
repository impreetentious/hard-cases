#!/usr/bin/env node

// Documentation is generated from `spec/v1`, never written alongside it.
//
// The rule this enforces is that neither implementation gets to be its own
// specification. If the prose describing the certificate format were maintained
// by hand, it would drift from the assets, and the first thing to drift would be
// whichever rule was hardest to satisfy. So the renderer here is deliberately
// mechanical: it has no editorial opinions, it cannot summarise, and a key added
// to an asset appears in the documentation whether or not anyone remembered to
// mention it.
//
// `--check` regenerates into a temporary directory and diffs, which is what CI
// runs. It also performs the checks that only make sense across assets: the
// placeholder scan, and the cross-references between rejection identifiers,
// vector files, and the traps they name.

import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const specDir = path.join(root, "spec", "v1");
const check = process.argv.includes("--check");

// ---------------------------------------------------------------------------
// The restricted TOML reader
// ---------------------------------------------------------------------------
//
// This accepts exactly the subset documented in spec/v1/README.md and rejects
// everything else. That strictness is the point: `hc-verify` cannot take a TOML
// dependency, so it will read these assets with a reader of about this size, and
// an asset this parser accepts but that one would not is an asset the verifier
// cannot read. Failing loudly here is how that stays true.

function parseSpec(text, file) {
  const result = {};
  let table = null;
  const fail = (line, message) => {
    throw new Error(`${file}:${line}: ${message}`);
  };

  const lines = text.split("\n");
  for (const [index, raw] of lines.entries()) {
    const lineNumber = index + 1;
    if (raw.endsWith("\r")) fail(lineNumber, "carriage return; assets are LF only");
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;

    if (line.startsWith("[")) {
      if (!line.endsWith("]")) fail(lineNumber, "malformed table header");
      if (line.startsWith("[[")) fail(lineNumber, "arrays of tables are outside the subset");
      const name = line.slice(1, -1);
      if (!/^[A-Za-z0-9_.-]+$/.test(name)) fail(lineNumber, `malformed table name: ${name}`);
      if (result[name]) fail(lineNumber, `duplicate table: ${name}`);
      table = {};
      result[name] = table;
      continue;
    }

    if (table === null) fail(lineNumber, "assignment before any table header");
    const split = line.indexOf("=");
    if (split < 0) fail(lineNumber, "line is neither a comment, a table header, nor an assignment");
    const key = line.slice(0, split).trim();
    const value = line.slice(split + 1).trim();
    if (!/^[A-Za-z0-9_-]+$/.test(key)) fail(lineNumber, `malformed key: ${key}`);
    if (key in table) fail(lineNumber, `duplicate key: ${key}`);
    table[key] = parseValue(value, lineNumber, fail);
  }
  return result;
}

function parseValue(value, lineNumber, fail) {
  if (value === "true") return true;
  if (value === "false") return false;
  if (/^-?[0-9]+$/.test(value)) return Number.parseInt(value, 10);
  if (value.startsWith("'")) {
    if (!value.endsWith("'") || value.length < 2) fail(lineNumber, "unterminated literal string");
    const inner = value.slice(1, -1);
    if (inner.includes("'")) fail(lineNumber, "literal string contains a quote");
    return inner;
  }
  if (value.startsWith("[")) {
    if (!value.endsWith("]")) fail(lineNumber, "array must close on the same line");
    const inner = value.slice(1, -1).trim();
    if (inner === "") return [];
    const items = [];
    // Literal strings contain no quotes and no escapes, so splitting on the
    // quote characters is exact rather than a heuristic.
    const pattern = /'([^']*)'\s*(,\s*)?/y;
    let position = 0;
    while (position < inner.length) {
      pattern.lastIndex = position;
      const match = pattern.exec(inner);
      if (!match) fail(lineNumber, "array elements must be literal strings");
      items.push(match[1]);
      position = pattern.lastIndex;
    }
    return items;
  }
  if (value.startsWith('"')) fail(lineNumber, "basic strings are outside the subset; use a literal string");
  if (value.startsWith("{")) fail(lineNumber, "inline tables are outside the subset");
  fail(lineNumber, `value is outside the subset: ${value}`);
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

function specFiles(dir) {
  const found = [];
  for (const entry of readdirSync(dir).sort()) {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) found.push(...specFiles(full));
    else if (entry.endsWith(".toml")) found.push(full);
  }
  return found;
}

const assets = new Map();
try {
  for (const file of specFiles(specDir)) {
    const relative = path.relative(specDir, file);
    assets.set(relative, parseSpec(readFileSync(file, "utf8"), relative));
  }
} catch (error) {
  // An asset outside the subset is a specification the verifier cannot read.
  // It is a failure of the same kind as a placeholder, and it reports the same
  // way rather than as a crash in the tooling.
  console.error("spec check FAILED:");
  console.error(` - ${error.message}`);
  process.exit(1);
}

// ---------------------------------------------------------------------------
// Cross-asset checks
// ---------------------------------------------------------------------------

const problems = [];

// The placeholder scan. A normative asset carrying an unfinished marker is a
// rule nobody has decided yet, wearing the clothes of one that has been.
const PLACEHOLDER = /\b(TODO|TBD|FIXME|XXX|placeholder|to be decided|to be determined)\b/i;
for (const file of [...specFiles(specDir), path.join(specDir, "README.md")]) {
  if (!existsSync(file)) continue;
  const relative = path.relative(specDir, file);
  for (const [index, line] of readFileSync(file, "utf8").split("\n").entries()) {
    if (PLACEHOLDER.test(line)) problems.push(`${relative}:${index + 1}: placeholder marker in a normative asset`);
  }
}

// Every asset states its version and title, so a rendered page can never be
// untitled and a consumer can always tell which contract it is holding.
for (const [file, asset] of assets) {
  const meta = asset.meta;
  if (!meta) problems.push(`${file}: no [meta] table`);
  else {
    if (meta.version !== 1) problems.push(`${file}: [meta] version must be 1 while the directory is v1`);
    for (const key of ["title", "summary"]) {
      if (typeof meta[key] !== "string") problems.push(`${file}: [meta] ${key} is missing`);
    }
  }
}

// Every rejection identifier mentioned anywhere must exist in the table, and
// every identifier in the table must belong to its declared group. The first
// check is what keeps a vector from naming a rule that was never written; the
// second keeps the table itself internally consistent.
const rejections = assets.get("rejections.toml");
const declared = new Set();
for (const [name, entry] of Object.entries(rejections)) {
  if (!name.startsWith("reject.")) continue;
  const id = name.slice("reject.".length);
  declared.add(id);
  if (!id.startsWith(`${entry.group}-`)) problems.push(`rejections.toml: ${id} does not belong to group ${entry.group}`);
  for (const key of ["check", "message"]) {
    if (typeof entry[key] !== "string") problems.push(`rejections.toml: ${id} is missing ${key}`);
  }
}

const ID_PATTERN = /\b(?:HCC|HC)-[A-Z]+-[0-9]{2}\b/g;
for (const [file, asset] of assets) {
  if (file === "rejections.toml") continue;
  for (const [name, entry] of Object.entries(asset)) {
    for (const value of Object.values(entry)) {
      for (const text of Array.isArray(value) ? value : [value]) {
        if (typeof text !== "string") continue;
        for (const id of text.match(ID_PATTERN) ?? []) {
          if (!declared.has(id)) problems.push(`${file}: [${name}] names undeclared rejection identifier ${id}`);
        }
      }
    }
  }
}

// Vector files referenced by a manifest must exist, because a manifest entry
// pointing at nothing is a test that silently covers nothing.
for (const [file, asset] of assets) {
  const dir = path.dirname(path.join(specDir, file));
  for (const [name, entry] of Object.entries(asset)) {
    for (const key of ["file", "canon", "instance"]) {
      const value = entry[key];
      if (typeof value !== "string") continue;
      if (!existsSync(path.resolve(dir, value))) {
        problems.push(`${file}: [${name}] ${key} points at a missing file: ${value}`);
      }
    }
  }
}

// Trap names are the contract between a rule and the test that proves it fires.
// They are spelled one way so a grep for a trap finds both ends of it.
for (const [file, asset] of assets) {
  for (const [name, entry] of Object.entries(asset)) {
    for (const key of ["trap", "traps"]) {
      const value = entry[key];
      if (value === undefined) continue;
      for (const trap of Array.isArray(value) ? value : [value]) {
        if (!/^trap_[a-z0-9_]+$/.test(trap)) problems.push(`${file}: [${name}] malformed trap name: ${trap}`);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

const PAGES = {
  "canonical.toml": "canonical-model.md",
  "mps.toml": "mps.md",
  "numeric.toml": "numeric-caps.md",
  "rng.toml": "determinism.md",
  "claims.toml": "claims.md",
  "hcc.toml": "certificate.md",
  "transforms.toml": "presolve-transforms.md",
  "cuts.toml": "cuts.md",
  "rejections.toml": "rejections.md",
};

const BANNER = [
  "<!--",
  "  Generated from spec/v1 by scripts/gen-docs.mjs. Do not edit.",
  "  The assets are the specification; this page is a rendering of them.",
  "-->",
  "",
].join("\n");

const escapeCell = (text) => String(text).replace(/\|/g, "\\|");

function renderValue(value) {
  if (Array.isArray(value)) return value.map((item) => `\n  - ${item}`).join("");
  if (typeof value === "boolean") return value ? "yes" : "no";
  return ` ${value}`;
}

function renderGeneric(asset) {
  const out = [];
  const meta = asset.meta;
  out.push(`# ${meta.title}`, "", meta.summary, "");
  for (const [name, entry] of Object.entries(asset)) {
    if (name === "meta") continue;
    out.push(`## \`[${name}]\``, "");
    for (const [key, value] of Object.entries(entry)) {
      out.push(`- **${key}** —${renderValue(value)}`);
    }
    out.push("");
  }
  return out.join("\n");
}

function renderRejections(asset) {
  const meta = asset.meta;
  const out = [`# ${meta.title}`, "", meta.summary, ""];
  for (const [name, entry] of Object.entries(asset)) {
    if (name === "meta" || name.startsWith("reject.")) continue;
    out.push(`## \`[${name}]\``, "");
    for (const [key, value] of Object.entries(entry)) out.push(`- **${key}** —${renderValue(value)}`);
    out.push("");
  }

  const groups = new Map();
  for (const [name, entry] of Object.entries(asset)) {
    if (!name.startsWith("reject.")) continue;
    const id = name.slice("reject.".length);
    if (!groups.has(entry.group)) groups.set(entry.group, []);
    groups.get(entry.group).push({ id, ...entry });
  }
  for (const [group, entries] of groups) {
    out.push(`## ${group}`, "", "| Identifier | Message | Earliest check |", "|---|---|---|");
    for (const entry of entries) {
      out.push(`| \`${entry.id}\` | ${escapeCell(entry.message)} | ${escapeCell(entry.check)} |`);
    }
    out.push("");
  }
  return out.join("\n");
}

function renderIndex() {
  const out = [
    "# Generated format documentation",
    "",
    "Every page here is rendered from the inert assets in `spec/v1` by",
    "`scripts/gen-docs.mjs`. Editing a page has no effect: continuous integration",
    "regenerates into a temporary directory and fails on any difference. Change the",
    "asset instead, and remember that a change to a normative rule also needs a",
    "decision record, a version bump on whichever contract moved, and regenerated",
    "fixtures.",
    "",
    "| Page | Asset | What it fixes |",
    "|---|---|---|",
  ];
  for (const [asset, page] of Object.entries(PAGES)) {
    const meta = assets.get(asset).meta;
    out.push(`| [${meta.title}](${page}) | \`spec/v1/${asset}\` | ${escapeCell(meta.summary)} |`);
  }
  out.push("");
  return out.join("\n");
}

function build() {
  const pages = new Map();
  for (const [asset, page] of Object.entries(PAGES)) {
    const body = asset === "rejections.toml" ? renderRejections(assets.get(asset)) : renderGeneric(assets.get(asset));
    pages.set(page, BANNER + body);
  }
  pages.set("README.md", BANNER + renderIndex());
  return pages;
}

function writeAll(target, pages) {
  mkdirSync(target, { recursive: true });
  for (const [name, body] of pages) writeFileSync(path.join(target, name), body, "utf8");
}

if (problems.length) {
  console.error("spec check FAILED:");
  for (const problem of problems) console.error(` - ${problem}`);
  process.exit(1);
}

const pages = build();
const target = path.join(root, "docs", "formats");

if (!check) {
  writeAll(target, pages);
  console.log(`gen-docs OK — ${pages.size} page(s) written to docs/formats`);
  process.exit(0);
}

const scratch = mkdtempSync(path.join(tmpdir(), "hc-docs-"));
try {
  writeAll(scratch, pages);
  const differences = [];
  for (const [name, body] of pages) {
    const existing = path.join(target, name);
    if (!existsSync(existing)) differences.push(`${name}: missing; run scripts/gen-docs.mjs`);
    else if (readFileSync(existing, "utf8") !== body) differences.push(`${name}: differs from the assets`);
  }
  if (existsSync(target)) {
    for (const name of readdirSync(target)) {
      if (!pages.has(name)) differences.push(`${name}: not generated by any asset; remove it`);
    }
  }
  if (differences.length) {
    console.error("gen-docs --check FAILED:");
    for (const difference of differences) console.error(` - ${difference}`);
    process.exit(1);
  }
  console.log(`gen-docs --check OK — ${pages.size} page(s) match spec/v1`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
