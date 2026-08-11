/**
 * Drift guard for postman/infraportal.postman_collection.json.
 *
 * The collection is a PUBLISHED artifact: anyone can import it into Postman
 * and read it. It is generated from the eleven per-service OpenAPI 3.0.3 specs
 * by generate-collection.mjs, and until this guard shipped nothing in CI ever
 * compared the two again. Measured on 2026-08-11: four commits had touched a
 * service openapi.yaml since the collection was generated (#116, #133, #136,
 * #137) and NONE of them regenerated it, so the committed collection had been
 * stale for 23 days while every check stayed green.
 *
 * This runs in .github/workflows/postman-ci.yml, which is paths-scoped to the
 * postman directory and to every openapi.yaml — so the author of a spec
 * change SEES the obligation as a red check on their own PR instead of being
 * expected to remember it.
 *
 * ── What it checks, and why each direction exists ──────────────────────────
 *
 *  1. FOLDER SET — the collection carries exactly one folder per discovered
 *     `<service>-service/openapi.yaml`, both directions. A new service whose
 *     spec lands without a regeneration is a missing folder; a service removed
 *     from the workspace leaves an orphan folder.
 *
 *  2. ROUTE SET — per service, the (method, path) pairs the spec declares and
 *     the ones the collection's requests issue are the same set, both
 *     directions. Derived from the SPEC TEXT, deliberately not by re-running
 *     the converter: a converter that silently dropped a route would produce a
 *     collection and a regeneration that agree with each other and are both
 *     wrong, which is exactly how infraportal's byte-comparing check-spec-drift
 *     stayed happy while all eleven snapshots carried a false claim (PR #116).
 *
 *  3. DESCRIPTION FRESHNESS — each folder's description begins with its spec's
 *     `info.description`. This is the check that catches the drift that
 *     actually existed: the route set was UNCHANGED (99 requests before and
 *     after regeneration), so a route-coverage check alone would have passed on
 *     a collection whose eleven folder descriptions all still told the reader
 *     "All runtime endpoints have been offline since 2026-06-04, when the
 *     platform infrastructure was decommissioned to zero" — a claim retired
 *     from the specs by PR #116 on 2026-07-25.
 *
 *  4. RETIRED RUNTIME-STATUS CLAIMS — that class of sentence appears nowhere in
 *     the collection or in the generator. Check 3 cannot reach the generator's
 *     own hardcoded collection-level description, which is not derived from any
 *     spec; that is precisely where the twelfth copy of the false claim lived.
 *     Bans the CLASS rather than the one sentence, the shape infraportal's
 *     specRuntimeStatus.test.ts settled on for the same artifact family.
 *
 * Hermetic on purpose: node builtins only, no npm install, no network, no
 * converter. It reads both real artifacts on every run and finishes in well
 * under a second.
 *
 * Usage: node postman/check-collection.mjs
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const collectionPath = path.join(here, "infraportal.postman_collection.json");
const generatorPath = path.join(here, "generate-collection.mjs");

/** Every workspace service today. A floor, not a fixture: see discoverServices. */
const MIN_SERVICES = 11;

const HTTP_METHODS = ["get", "post", "put", "patch", "delete"];

/**
 * Sentences asserting a RUNTIME STATUS, which a contract document must not do:
 * they rot the moment the platform changes and are read by end users. The
 * source of truth is the live status board linked from every spec.
 */
const RETIRED_STATUS_CLAIMS = [
  /decommissioned to zero/i,
  /endpoints have been offline/i,
  /infrastructure (?:has been |was )?decommissioned/i,
];

const failures = [];
function fail(message) {
  failures.push(message);
}

/**
 * Reads a file, turning an unreadable path into an explicit failure line rather
 * than an empty string. An unreadable input is a THIRD state — neither "clean"
 * nor "found a problem" — and a guard that silently treats it as the first
 * reports health about something it never read.
 */
function read(file) {
  try {
    return readFileSync(file, "utf8");
  } catch (err) {
    fail(`CANNOT-READ ${path.relative(repoRoot, file)}: ${err.message}`);
    return null;
  }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/**
 * Discovers services by globbing `<name>-service/openapi.yaml` under the repo
 * root. Hand-enumerating them would degrade silently: a twelfth service would
 * simply never be looked for, which is the one failure this guard exists to
 * catch. Hard-fails below the known floor so a broken walk can never present
 * itself as a small, clean corpus.
 */
function discoverServices() {
  let entries;
  try {
    entries = readdirSync(repoRoot, { withFileTypes: true });
  } catch (err) {
    console.error(`[postman-check] FATAL: cannot list ${repoRoot}: ${err.message}`);
    process.exit(1);
  }
  const found = [];
  for (const entry of entries) {
    if (!entry.isDirectory() || !entry.name.endsWith("-service")) continue;
    const spec = path.join(repoRoot, entry.name, "openapi.yaml");
    try {
      if (statSync(spec).isFile()) found.push({ folder: entry.name, spec });
    } catch {
      // A `*-service` directory with no spec is not part of the published
      // surface; the floor assertion below is what catches a broken walk.
    }
  }
  found.sort((a, b) => a.folder.localeCompare(b.folder));
  if (found.length < MIN_SERVICES) {
    console.error(
      `[postman-check] FATAL: discovered ${found.length} '*-service/openapi.yaml' specs under ` +
        `${repoRoot}, expected at least ${MIN_SERVICES}. Either a spec is missing or this ` +
        `scan is broken — refusing to report a verdict on a corpus this small.`,
    );
    process.exit(1);
  }
  return found;
}

// ---------------------------------------------------------------------------
// Spec parsing (line-anchored, the shape the service role-gating suites use)
// ---------------------------------------------------------------------------

/**
 * Returns the lines of the top-level `paths:` block, asserting the anchor
 * appears exactly once at column 0 and that the block is non-empty. A reworded
 * or restructured spec must redden this guard, never silently empty its corpus.
 */
function pathsBlock(spec, source) {
  const lines = source.split(/\r?\n/);
  const anchors = lines.filter((l) => l.trimEnd() === "paths:").length;
  if (anchors !== 1) {
    fail(`${spec}: expected exactly one top-level 'paths:' anchor, found ${anchors}`);
    return [];
  }
  const start = lines.findIndex((l) => l.trimEnd() === "paths:") + 1;
  const block = [];
  for (let i = start; i < lines.length; i++) {
    const line = lines[i];
    // The block ends at the next column-0 key (`components:`, `tags:`, ...).
    if (line !== "" && !line.startsWith(" ")) break;
    block.push(line);
  }
  if (block.every((l) => l.trim() === "")) {
    fail(`${spec}: the 'paths:' block parsed as empty — the spec's shape changed`);
    return [];
  }
  return block;
}

/**
 * Every (method, path) pair the spec declares. Path keys sit at exactly two
 * spaces of indent and start with `/`; method keys sit at exactly four.
 */
function specOperations(spec, source) {
  const ops = new Set();
  let current = null;
  for (const raw of pathsBlock(spec, source)) {
    const line = raw.trimEnd();
    if (line === "") continue;
    const pathKey = /^ {2}(\/\S*):$/.exec(line);
    if (pathKey) {
      current = pathKey[1];
      continue;
    }
    const methodKey = /^ {4}([a-z]+):$/.exec(line);
    if (methodKey && HTTP_METHODS.includes(methodKey[1])) {
      if (current === null) {
        fail(`${spec}: method key '${methodKey[1]}' appeared before any path key`);
        continue;
      }
      ops.add(`${methodKey[1].toUpperCase()} ${current}`);
    }
  }
  if (ops.size === 0) {
    fail(`${spec}: no operations parsed out of the 'paths:' block`);
  }
  return ops;
}

/**
 * The spec's `info.description` block scalar, dedented. Returns null (and fails)
 * when it cannot be read, so a restructured `info:` block reddens rather than
 * comparing against an empty string that every folder trivially starts with.
 */
function specDescription(spec, source) {
  const lines = source.split(/\r?\n/);
  const infoIdx = lines.findIndex((l) => l.trimEnd() === "info:");
  if (infoIdx < 0) {
    fail(`${spec}: no top-level 'info:' key`);
    return null;
  }
  for (let i = infoIdx + 1; i < lines.length; i++) {
    const line = lines[i];
    if (line.trim() === "") continue;
    if (!line.startsWith(" ")) break; // left the info: block
    if (!/^ {2}description:\s*[|>]-?\s*$/.test(line)) continue;
    const body = [];
    for (let j = i + 1; j < lines.length; j++) {
      const bodyLine = lines[j];
      if (bodyLine.trim() === "") {
        body.push("");
        continue;
      }
      if (!/^ {4}/.test(bodyLine)) break;
      body.push(bodyLine.slice(4));
    }
    const text = body.join("\n").replace(/\n+$/, "");
    if (text === "") {
      fail(`${spec}: 'info.description' block scalar parsed as empty`);
      return null;
    }
    return text;
  }
  fail(`${spec}: no 'info.description' block scalar found under 'info:'`);
  return null;
}

// ---------------------------------------------------------------------------
// Collection parsing
// ---------------------------------------------------------------------------

/** Every leaf request under a folder, as "<METHOD> /<path>". */
function folderRequests(items, out) {
  for (const entry of items || []) {
    if (Array.isArray(entry.item)) folderRequests(entry.item, out);
    if (!entry.request) continue;
    const url = entry.request.url || {};
    const segments = Array.isArray(url.path) ? url.path : [];
    out.push({
      name: entry.name,
      key: `${String(entry.request.method || "").toUpperCase()} /${segments.join("/")}`,
    });
  }
  return out;
}

/** OpenAPI templating (`{id}`) in the collection's vocabulary (`:id`). */
function toPostmanPath(operation) {
  return operation.replace(/\{([^}]+)\}/g, ":$1");
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

const services = discoverServices();

const collectionSource = read(collectionPath);
const generatorSource = read(generatorPath);
if (collectionSource === null || generatorSource === null) {
  console.error(`[postman-check] FAIL\n  - ${failures.join("\n  - ")}`);
  process.exit(1);
}

let collection;
try {
  collection = JSON.parse(collectionSource);
} catch (err) {
  console.error(`[postman-check] FATAL: infraportal.postman_collection.json is not valid JSON: ${err.message}`);
  process.exit(1);
}

const folders = Array.isArray(collection.item) ? collection.item : [];
if (folders.length === 0) {
  console.error("[postman-check] FATAL: the collection declares no folders at all");
  process.exit(1);
}

// ── 1. Folder set ───────────────────────────────────────────────────────────
const specFolderNames = new Set(services.map((s) => s.folder));
const collectionFolderNames = new Set(folders.map((f) => f.name));
for (const name of specFolderNames) {
  if (!collectionFolderNames.has(name)) {
    fail(
      `the collection has no folder for '${name}', whose openapi.yaml exists — ` +
        `regenerate with 'node postman/generate-collection.mjs'`,
    );
  }
}
for (const name of collectionFolderNames) {
  if (!specFolderNames.has(name)) {
    fail(`the collection carries a folder '${name}' with no matching '<service>/openapi.yaml'`);
  }
}

// ── 2. Route set, 3. Description freshness ──────────────────────────────────
let totalOperations = 0;
let totalRequests = 0;
let descriptionsChecked = 0;

for (const { folder, spec } of services) {
  const source = read(spec);
  if (source === null) continue;
  const relSpec = path.relative(repoRoot, spec).split(path.sep).join("/");

  const declared = new Set([...specOperations(relSpec, source)].map(toPostmanPath));
  totalOperations += declared.size;

  const node = folders.find((f) => f.name === folder);
  if (!node) continue; // already reported by check 1

  const requests = folderRequests(node.item, []);
  totalRequests += requests.length;
  if (requests.length === 0) {
    fail(`${folder}: the collection folder contains no requests at all`);
  }

  const issued = new Set(requests.map((r) => r.key));
  for (const op of declared) {
    if (!issued.has(op)) {
      fail(
        `${relSpec} declares '${op}' but no request in the '${folder}' folder issues it — ` +
          `the collection is stale; regenerate it`,
      );
    }
  }
  for (const request of requests) {
    if (!declared.has(request.key)) {
      fail(
        `${folder}: request '${request.name}' issues '${request.key}', which ${relSpec} does not declare — ` +
          `the route was renamed or removed; regenerate the collection`,
      );
    }
  }

  // ── 3 ──
  const wanted = specDescription(relSpec, source);
  const got = node.description && typeof node.description.content === "string" ? node.description.content : null;
  if (wanted !== null) {
    if (got === null) {
      fail(`${folder}: the collection folder carries no description, but ${relSpec} declares one`);
    } else if (!got.startsWith(wanted)) {
      descriptionsChecked += 1;
      let at = 0;
      while (at < wanted.length && at < got.length && wanted[at] === got[at]) at += 1;
      fail(
        `${folder}: the folder description no longer matches ${relSpec}'s info.description ` +
          `(they diverge at character ${at}) — the spec's prose changed and the collection was ` +
          `never regenerated.\n      spec:       ${JSON.stringify(wanted.slice(at, at + 90))}\n` +
          `      collection: ${JSON.stringify(got.slice(at, at + 90))}`,
      );
    } else {
      descriptionsChecked += 1;
    }
  }
}

// ── 4. Retired runtime-status claims ────────────────────────────────────────
for (const [label, text] of [
  ["infraportal.postman_collection.json", collectionSource],
  ["generate-collection.mjs", generatorSource],
]) {
  for (const pattern of RETIRED_STATUS_CLAIMS) {
    const hits = text.match(new RegExp(pattern.source, "gi"));
    if (hits) {
      fail(
        `${label} asserts a RUNTIME STATUS (${hits.length} match(es) of ${pattern}) — a contract ` +
          `document must not; live per-service health belongs on the status board at ` +
          `https://rodmen07.github.io/infraportal/#/status`,
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

if (failures.length > 0) {
  console.error(`[postman-check] FAIL: ${failures.length} problem(s)\n  - ${failures.join("\n  - ")}`);
  console.error(
    "\n[postman-check] The collection is generated. Fix the specs if they are wrong, then run\n" +
      "                node postman/generate-collection.mjs\n" +
      "                and commit the regenerated postman/infraportal.postman_collection.json.",
  );
  process.exit(1);
}

console.log(
  `[postman-check] OK: ${services.length} specs, ${folders.length} folders, ` +
    `${totalOperations} declared operations matched by ${totalRequests} requests, ` +
    `${descriptionsChecked} folder descriptions in sync, ` +
    `0 retired runtime-status claims.`,
);
