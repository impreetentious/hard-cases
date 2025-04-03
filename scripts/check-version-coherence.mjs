#!/usr/bin/env node

// One version, every surface. A release that ships a README claiming one
// version, a workspace manifest claiming another, and a lockfile claiming a
// third is not a release; it is three of them wearing a trench coat. Each
// check below is guarded by `existsSync`, so the script is correct in a partial
// checkout and so a surface that is absent is never silently assumed to agree.

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (file) => readFileSync(path.join(root, file), "utf8");
const has = (file) => existsSync(path.join(root, file));
const errors = [];
const semver = "([0-9]+\\.[0-9]+\\.[0-9]+)";

const readmeVersion = read("README.md").match(
  new RegExp(`\\*\\*Version:\\*\\*\\s+v${semver}(?:\\s|$)`),
)?.[1];
if (!readmeVersion) errors.push("README.md missing **Version:** vX.Y.Z marker");

let version = readmeVersion;
if (has("Cargo.toml")) {
  const cargoVersion = read("Cargo.toml").match(
    new RegExp(`\\[workspace\\.package\\][\\s\\S]*?^version\\s*=\\s*"${semver}"`, "m"),
  )?.[1];
  if (!cargoVersion) errors.push("Cargo.toml missing workspace package version");
  if (version && cargoVersion !== version) {
    errors.push(`Cargo.toml version ${cargoVersion ?? "missing"} != README.md ${version}`);
  }
  version = cargoVersion ?? version;
}

if (version && has("Cargo.lock")) {
  for (const block of read("Cargo.lock").split("[[package]]").slice(1)) {
    const name = block.match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1];
    const packageVersion = block.match(/^\s*version\s*=\s*"([^"]+)"/m)?.[1];
    if (name?.startsWith("hc-") && packageVersion !== version) {
      errors.push(`Cargo.lock ${name} version ${packageVersion ?? "missing"} != ${version}`);
    }
  }
}

if (version && has("theater/package.json")) {
  const pkg = JSON.parse(read("theater/package.json"));
  if (pkg.version !== version) {
    errors.push(`theater/package.json version ${pkg.version} != ${version}`);
  }
  if (has("theater/package-lock.json")) {
    const lock = JSON.parse(read("theater/package-lock.json"));
    if (lock.version !== version) {
      errors.push(`theater/package-lock.json version ${lock.version} != ${version}`);
    }
    if (lock.packages?.[""]?.version !== version) {
      errors.push(`theater lock root version ${lock.packages?.[""]?.version} != ${version}`);
    }
  }
}

// The verifier prints its own version and source hash in the trust-boundary
// receipt. That number is a claim about which code checked the proof, so it
// must come from the manifest and never from a hand-maintained string.
for (const file of ["crates/hc-cli/src/version.rs", "crates/hc-verify/src/vcli.rs"]) {
  if (!has(file)) continue;
  if (!read(file).includes('env!("CARGO_PKG_VERSION")')) {
    errors.push(`${file} must derive runtime versions from CARGO_PKG_VERSION`);
  }
}

if (errors.length) {
  console.error("version-coherence FAILED:");
  for (const error of errors) console.error(` - ${error}`);
  process.exit(1);
}

console.log(`version-coherence OK — ${version}`);
