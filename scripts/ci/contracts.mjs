// The contracts the specification names are real ones.
//
// Two of them, and both are promises the assets make to a reader.
//
// **Every published rejection identifier is one some implementation can
// produce.**
//
// A rejection identifier is a public interface: `hc verify` prints it, the
// tamper corpus pairs every mutation with the one that must answer it, the
// situation room shows it, and the case files cite it. An identifier declared
// in the frozen table that no code can emit is a rule the specification
// promises and nothing enforces — and a reader auditing this project by
// reading that table would be auditing a list of intentions.
//
// Eight of a hundred and twenty-two were exactly that when this was written.
// None of them was a hole in the verifier: each named a check an earlier
// identifier already answered, or an encoding this format does not have. They
// are marked retired in the asset, which is what "append-only" means here, and
// this refuses the build if one comes back or if a live one goes missing.
//
// The scan is over the two hand-written registries and over every use site.
// Registration is not enough: a constant nobody names is an identifier nobody
// can be answered with.

import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

// The asset is read with a regular expression rather than through the subset
// reader the other tools share. What this needs from it is a list of table
// names and one boolean per table, the shape of both is fixed by the asset's
// own grammar, and a second parser here would be a second thing to keep in
// step for no gain. A malformed table is caught by the readers that do parse
// it, in the same preflight.
const table = readFileSync(path.join(root, "spec", "v1", "rejections.toml"), "utf8");
const declared = new Map();
for (const block of table.split(/\n(?=\[)/)) {
  const name = /^\[reject\.([A-Z][A-Z0-9-]*)\]/.exec(block);
  if (!name) continue;
  declared.set(name[1], /^retired = true$/m.test(block));
}

/** The two hand-written registries, as identifier to constant name. */
const registries = [
  ["crates/hc-core/src/reject.rs", /^\s+([A-Z][A-Z0-9_]*) => "([A-Z-]+-\d\d)",/gm],
  ["crates/hc-verify/src/vreject.rs", /^\s+([A-Z][A-Z0-9_]*) = "([A-Z-]+-\d\d)",/gm],
];
const registered = new Map();
const registryFiles = new Set();
for (const [file, pattern] of registries) {
  registryFiles.add(path.join(root, file));
  const text = readFileSync(path.join(root, file), "utf8");
  for (const [, constant, id] of text.matchAll(pattern)) {
    if (!registered.has(id)) registered.set(id, new Set());
    registered.get(id).add(constant);
  }
}

/** Every constant named anywhere outside the registries. */
const named = new Set();
function walk(from) {
  for (const entry of readdirSync(from, { withFileTypes: true })) {
    if (entry.name === "target" || entry.name === ".git") continue;
    const full = path.join(from, entry.name);
    if (entry.isDirectory()) {
      walk(full);
      continue;
    }
    if (!entry.name.endsWith(".rs") || registryFiles.has(full)) continue;
    const text = readFileSync(full, "utf8");
    for (const [, constant] of text.matchAll(/\b(?:Code|Rejection)::([A-Z][A-Z0-9_]*)\b/g)) {
      named.add(constant);
    }
  }
}
walk(path.join(root, "crates"));
if (statSync(path.join(root, "crates")).isDirectory() === false) {
  console.error("rejections: FAILED — there are no crates to scan");
  process.exit(1);
}

const failures = [];
let live = 0;
let retired = 0;
for (const [id, isRetired] of declared) {
  const constants = registered.get(id) ?? new Set();
  if (isRetired) {
    retired += 1;
    if (constants.size > 0) {
      failures.push(
        `${id} is retired and is registered as ${[...constants].join(", ")}; remove the constant or un-retire the identifier`,
      );
    }
    continue;
  }
  live += 1;
  if (constants.size === 0) {
    failures.push(`${id} is declared and no implementation registers it`);
    continue;
  }
  if (![...constants].some((constant) => named.has(constant))) {
    failures.push(
      `${id} is registered as ${[...constants].join(", ")} and nothing names it, so nothing can answer with it`,
    );
  }
}
for (const [id] of registered) {
  if (!declared.has(id)) {
    failures.push(`${id} is registered and the frozen table does not declare it`);
  }
}
if (live === 0 || retired === 0) {
  failures.push("the table has no live or no retired identifiers; this check covers nothing");
}

if (failures.length) {
  console.error("contracts: FAILED");
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}
// ---------------------------------------------------------------------------
// The traps
// ---------------------------------------------------------------------------
//
// **Every trap an asset names is a test of that name.** A trap is the contract
// between a rule and the test that proves it fires, and the assets say the two
// are spelled one way so that a grep finds both ends of it. Twenty-two of the
// thirty-eight found only one end when this was written: the properties were
// covered, by tests named as sentences, so a reader following the contract
// landed nowhere.
//
// The reverse is deliberately not checked. A test named in the convention that
// no asset declares is a property the implementation pins for itself — the
// search is not specified by an inert asset and is not supposed to be, because
// nothing it does is a claim. Requiring an asset entry for those would mean
// writing a specification for the part of this project that deliberately has
// none.

const declaredTraps = new Set();
function assets(from) {
  for (const entry of readdirSync(from, { withFileTypes: true })) {
    const full = path.join(from, entry.name);
    if (entry.isDirectory()) {
      assets(full);
      continue;
    }
    if (!entry.name.endsWith(".toml")) continue;
    const text = readFileSync(full, "utf8");
    for (const [, value] of text.matchAll(/traps?\s*=\s*(\[[^\]]*\]|'[^']*')/g)) {
      for (const [, trap] of value.matchAll(/'(trap_[a-z0-9_]+)'/g)) declaredTraps.add(trap);
    }
  }
}
assets(path.join(root, "spec", "v1"));

const testNames = new Set();
function tests(from) {
  for (const entry of readdirSync(from, { withFileTypes: true })) {
    if (entry.name === "target" || entry.name === ".git") continue;
    const full = path.join(from, entry.name);
    if (entry.isDirectory()) {
      tests(full);
      continue;
    }
    if (!entry.name.endsWith(".rs")) continue;
    for (const [, name] of readFileSync(full, "utf8").matchAll(/\bfn\s+(trap_[a-z0-9_]+)/g)) {
      testNames.add(name);
    }
  }
}
tests(path.join(root, "crates"));

const orphans = [];
for (const trap of declaredTraps) {
  if (!testNames.has(trap)) orphans.push(`${trap} is named by an asset and no test carries that name`);
}
if (declaredTraps.size === 0) orphans.push("no asset names a trap; this check covers nothing");
if (orphans.length) {
  console.error("contracts: FAILED");
  for (const orphan of orphans) console.error(`  ${orphan}`);
  process.exit(1);
}

console.log(
  `contracts: OK — ${live} live rejection identifier(s) every one of which some implementation can produce, ${retired} retired and registered by nobody, ${declaredTraps.size} trap(s) every one of which names a test`,
);
