// Every relative link in every document points at something that exists.
//
// This exists because the README named six essays, a demo script, a recorded
// animation and two instance files, none of which had been written. Each was a
// reasonable intention when it was typed and a false statement by the time
// anybody read it, which is a bad thing for the front page of a project whose
// whole subject is not claiming what it cannot show.
//
// A link is checked as a path and nothing more: no network, no anchors
// resolved inside the target, no rendering. That is deliberately shallow, and
// it is the half that goes wrong.

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const skip = new Set(["node_modules", "target", "dist", ".git", "out", "artifacts"]);

/** Every Markdown document in the repository. */
function documents(from) {
  const out = [];
  for (const entry of readdirSync(from, { withFileTypes: true })) {
    if (skip.has(entry.name)) continue;
    const full = path.join(from, entry.name);
    if (entry.isDirectory()) out.push(...documents(full));
    else if (entry.name.endsWith(".md")) out.push(full);
  }
  return out;
}

const failures = [];
let checked = 0;

for (const file of documents(root)) {
  const text = readFileSync(file, "utf8");
  // Fenced code is handled separately below: a Markdown link inside a shell
  // example is not a link, and a path inside one is a promise of a different
  // kind — that the command a reader is about to copy will find its file.
  const prose = text.replace(/```[\s\S]*?```/g, "");
  for (const match of prose.matchAll(/\[[^\]]*\]\(([^)\s]+)\)/g)) {
    const target = match[1];
    if (/^(https?:|mailto:|#)/.test(target)) continue;
    checked += 1;
    const [where] = target.split("#");
    if (where === "") continue;
    const resolved = path.resolve(path.dirname(file), where);
    if (!existsSync(resolved)) {
      failures.push(`${path.relative(root, file)} → ${target}`);
      continue;
    }
    // A link ending in a slash is a directory and a link that does not is not.
    // The distinction matters on a static host, where the difference between
    // the two is a redirect that may or may not happen.
    const directory = statSync(resolved).isDirectory();
    if (directory !== target.endsWith("/")) {
      failures.push(
        `${path.relative(root, file)} → ${target} is ${
          directory ? "a directory and the link has no trailing slash" : "a file and the link has one"
        }`,
      );
    }
  }

  // The paths inside shell examples. A README that tells a reader to solve
  // `instances/demo/sudoku16.mps` and has no such file wastes the first thing
  // they try, and that is exactly what this repository's front page did.
  //
  // Only arguments that look like paths into this repository are checked:
  // anything with a slash and a known extension, no leading dash, no scheme.
  // A flag, a bare command, and a file the reader is being told to create are
  // all left alone, because the promise is about what is here.
  for (const fence of text.matchAll(/```(?:sh|bash|console)?\n([\s\S]*?)```/g)) {
    for (const word of fence[1].split(/\s+/)) {
      if (!/^[A-Za-z0-9_./-]+\/[A-Za-z0-9_.-]+\.[A-Za-z0-9]+$/.test(word)) continue;
      if (word.startsWith("-") || word.includes("://")) continue;
      if (word.startsWith("out/") || word.startsWith("target/")) continue;
      checked += 1;
      const resolved = path.resolve(root, word.replace(/^\.\//, ""));
      if (!existsSync(resolved)) {
        failures.push(`${path.relative(root, file)} → ${word} (inside a shell example)`);
      }
    }
  }
}

if (failures.length) {
  console.error("links: FAILED");
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}
console.log(`links: OK — ${checked} relative link(s) in the documentation resolve`);
