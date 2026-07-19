/**
 * Regenerates postman/infraportal.postman_collection.json from the eleven
 * per-service OpenAPI 3.0.3 specs at <repo root>/<service>-service/openapi.yaml.
 *
 * Each spec is converted with a pinned `npx --yes -p openapi-to-postmanv2@4
 * openapi2postmanv2` invocation (the package's CLI binary is named
 * openapi2postmanv2), then the per-service collections are merged into one
 * Postman Collection v2.1 file with:
 *
 *   - one folder per service,
 *   - collection-level bearer auth reading a {{token}} variable,
 *   - a single {{baseUrl}} collection variable used by every request URL
 *     (default http://localhost:8080, the local go-gateway).
 *
 * Deterministic: volatile converter output (uuid `id` / `_postman_id` keys) is
 * stripped, the converter's schema faker is made reproducible by preloading a
 * fixed-seed Math.random into the CLI process (its enum picks are otherwise
 * random per run), and all object keys are serialized in sorted order, so
 * reruns over unchanged specs are byte-identical and diff-stable.
 *
 * Usage: node postman/generate-collection.mjs
 */
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const outPath = path.join(here, "infraportal.postman_collection.json");

const services = [
  "accounts",
  "activities",
  "audit",
  "automation",
  "contacts",
  "integrations",
  "opportunities",
  "projects",
  "reporting",
  "search",
  "spend",
];

// ---------------------------------------------------------------------------
// Preflight: every spec must exist before any conversion starts.
// ---------------------------------------------------------------------------
const missing = services
  .map((s) => path.join(repoRoot, `${s}-service`, "openapi.yaml"))
  .filter((p) => !existsSync(p));
if (missing.length > 0) {
  console.error("[postman] FATAL: missing OpenAPI specs:");
  for (const p of missing) console.error(`  - ${p}`);
  process.exit(1);
}

// Resolve npx next to the node binary running this script, so the script works
// even when the nodejs directory is not on PATH (a recurring gotcha in this
// repo's Bash tool environment). Falls back to plain `npx` from PATH.
const npxSibling = path.join(
  path.dirname(process.execPath),
  process.platform === "win32" ? "npx.cmd" : "npx",
);
const npx = existsSync(npxSibling) ? `"${npxSibling}"` : "npx";

// ---------------------------------------------------------------------------
// Convert each spec into a temp dir with the pinned converter.
// ---------------------------------------------------------------------------
const tmpDir = mkdtempSync(path.join(os.tmpdir(), "infraportal-postman-"));
const converted = new Map(); // service -> parsed per-service collection

// The converter fills schema-derived example values with its schema faker,
// which picks enum members via Math.random, so unpinned runs differ (for
// example "status": "active" vs "inactive"). Preload a fixed-seed xorshift32
// PRNG over Math.random in the CLI process to make every run identical.
const preloadPath = path.join(tmpDir, "pin-random.cjs");
writeFileSync(
  preloadPath,
  [
    "// Deterministic Math.random (fixed-seed xorshift32) so the",
    "// openapi-to-postmanv2 schema faker produces identical output per run.",
    "let state = 0x1f2e3d4c;",
    "Math.random = function () {",
    "  state ^= state << 13;",
    "  state ^= state >>> 17;",
    "  state ^= state << 5;",
    "  return (state >>> 0) / 0x100000000;",
    "};",
    "",
  ].join("\n"),
);
// Inside a quoted NODE_OPTIONS value Node treats backslash as an escape
// character, so a quoted Windows path silently loses its separators. Forward
// slashes are safe on every platform and survive quoting.
const preloadArg = preloadPath.split(path.sep).join("/");
const childEnv = {
  ...process.env,
  NODE_OPTIONS: `${process.env.NODE_OPTIONS || ""} --require "${preloadArg}"`.trim(),
};

try {
  for (const service of services) {
    // The converter reads specs via fs, but keep the spec path relative with
    // forward slashes (cwd at repo root) to sidestep Windows absolute-path
    // quoting issues; the temp output may be on another drive, so it stays
    // absolute and quoted.
    const specRel = `${service}-service/openapi.yaml`;
    const tmpOut = path.join(tmpDir, `${service}.postman.json`);

    console.log(`[postman] converting ${specRel}`);
    const result = spawnSync(
      `${npx} --yes -p openapi-to-postmanv2@4 openapi2postmanv2 ` +
        `-s "${specRel}" -o "${tmpOut}" ` +
        `-O folderStrategy=Tags,parametersResolution=Example`,
      { cwd: repoRoot, shell: true, stdio: ["ignore", "pipe", "pipe"], encoding: "utf8", env: childEnv },
    );
    if (result.status !== 0 || !existsSync(tmpOut)) {
      console.error(`[postman] FATAL: conversion failed for ${specRel} (exit ${result.status})`);
      console.error(result.stderr || result.stdout || "");
      process.exit(1);
    }
    converted.set(service, JSON.parse(readFileSync(tmpOut, "utf8")));
  }
} finally {
  rmSync(tmpDir, { recursive: true, force: true });
}

// ---------------------------------------------------------------------------
// Merge into one collection.
// ---------------------------------------------------------------------------

/** Recursively drop volatile keys the converter regenerates on every run. */
function stripVolatile(node) {
  if (Array.isArray(node)) {
    for (const entry of node) stripVolatile(entry);
  } else if (node && typeof node === "object") {
    delete node.id;
    delete node._postman_id;
    delete node.uid;
    for (const value of Object.values(node)) stripVolatile(value);
  }
  return node;
}

/** Recursively rewrite the converter's {{bearerToken}} placeholder to {{token}}. */
function renameTokenVariable(node) {
  if (typeof node === "string") return node.replaceAll("{{bearerToken}}", "{{token}}");
  if (Array.isArray(node)) return node.map(renameTokenVariable);
  if (node && typeof node === "object") {
    for (const [key, value] of Object.entries(node)) node[key] = renameTokenVariable(value);
  }
  return node;
}

/** Count leaf requests and assert every request URL is rooted at {{baseUrl}}. */
function auditItems(items, service, counter) {
  for (const entry of items) {
    if (Array.isArray(entry.item)) {
      auditItems(entry.item, service, counter);
    } else if (entry.request) {
      counter.count += 1;
      const host = entry.request.url && entry.request.url.host;
      if (!Array.isArray(host) || host.join("") !== "{{baseUrl}}") {
        console.error(
          `[postman] FATAL: ${service}: request "${entry.name}" is not rooted at {{baseUrl}} ` +
            `(host: ${JSON.stringify(host)})`,
        );
        process.exit(1);
      }
    }
  }
}

const folders = [];
const perServiceCounts = new Map();
for (const service of services) {
  const col = renameTokenVariable(stripVolatile(converted.get(service)));
  const counter = { count: 0 };
  auditItems(col.item, service, counter);
  if (counter.count === 0) {
    console.error(`[postman] FATAL: ${service}: converted collection contains no requests`);
    process.exit(1);
  }
  perServiceCounts.set(service, counter.count);
  folders.push({
    name: `${service}-service`,
    description: col.info && col.info.description ? col.info.description : undefined,
    item: col.item,
  });
}

const collection = {
  info: {
    name: "InfraPortal Platform API",
    description: {
      content:
        "Postman collection for the InfraPortal CRM platform (v1.16.3 PR2), " +
        "generated from the eleven per-service OpenAPI 3.0.3 specs by " +
        "postman/generate-collection.mjs. Do not edit by hand; edit the specs " +
        "and regenerate.\n\n" +
        "All runtime endpoints have been offline since 2026-06-04 " +
        "(infrastructure decommissioned to zero); the collection documents " +
        "the API contract as implemented in code. Point {{baseUrl}} at a " +
        "locally running stack (default http://localhost:8080, the local " +
        "go-gateway) and supply a dev JWT in {{token}}. See postman/README.md " +
        "and docs/API.md.",
      type: "text/plain",
    },
    schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
  },
  auth: {
    type: "bearer",
    bearer: [{ key: "token", value: "{{token}}", type: "string" }],
  },
  variable: [
    {
      key: "baseUrl",
      value: "http://localhost:8080",
      type: "string",
      description:
        "Base URL every request is rooted at. Default is the local go-gateway; " +
        "override per environment (see infraportal.postman_environment.json).",
    },
  ],
  item: folders,
};

// ---------------------------------------------------------------------------
// Serialize with sorted keys for byte-stable, diff-friendly output.
// ---------------------------------------------------------------------------
function canonicalize(node) {
  if (Array.isArray(node)) return node.map(canonicalize);
  if (node && typeof node === "object") {
    const sorted = {};
    for (const key of Object.keys(node).sort()) {
      if (node[key] !== undefined) sorted[key] = canonicalize(node[key]);
    }
    return sorted;
  }
  return node;
}

writeFileSync(outPath, JSON.stringify(canonicalize(collection), null, 2) + "\n");

let total = 0;
console.log("\n[postman] request counts per service folder:");
for (const [service, count] of perServiceCounts) {
  total += count;
  console.log(`  ${service}-service: ${count}`);
}
console.log(`[postman] OK: ${folders.length} folders, ${total} requests -> ${outPath}`);
