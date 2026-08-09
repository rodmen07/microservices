# @rodmen07/infraportal-sdk

TypeScript SDK for the InfraPortal CRM platform API. A small hand-written fetch client plus types generated from the eleven per-service OpenAPI 3.0.3 specs in this repo.

> **Runtime status:** the platform services are deployed on Google Cloud Run, and live per-service health is published on the [platform status board](https://rodmen07.github.io/infraportal/#/status). This SDK targets the documented API contract, as captured in the per-service specs and verified by the service test suites. It also works against a locally run service or gateway.

> **Publishing:** the package is licensed MIT (`LICENSE` sits at the package root so npm bundles it from this subdirectory) and its manifest is publish-ready, but nothing has been published yet — `npm view @rodmen07/infraportal-sdk` returns 404. The earlier note here said the license field was "intentionally absent until a license is chosen"; that stopped being true when the repo adopted MIT.

## Layout

```
sdks/typescript-sdk/
  scripts/generate.mjs   type generation driver (pinned openapi-typescript@6)
  src/core/client.ts     hand-written fetch client: auth, errors, rate limits, retry
  src/core/routes.ts     route pinning + path expansion shared by the service modules
  src/generated/*.ts     one generated types file per service spec (do not edit)
  src/services/*.ts      one typed service module per covered service
  src/index.ts           package entry point
  tests/*.test.mjs       node:test unit tests (no network access)
```

**All eleven services have a typed module:** `AccountsApi`, `ActivitiesApi`, `AuditApi`,
`AutomationApi`, `ContactsApi`, `IntegrationsApi`, `OpportunitiesApi`, `ProjectsApi`,
`ReportingApi`, `SearchApi` and `SpendApi`.

Every module declares its routes as literals pinned to that service's generated `paths` type, so
a route renamed in an `openapi.yaml` fails the build instead of silently calling a dead URL, and a
route *added* to a spec fails the build naming the path no module covers. Response types are
derived from each operation's own declared success response rather than from a schema name —
which matters, because `list` returns a paginated envelope on accounts, contacts, audit and spend
and a bare array on activities, automation, integrations, opportunities, projects and reporting.
See `src/core/routes.ts` for what that pinning does and does not prove.

Several surfaces are deliberately not uniform, and each is pinned rather than remembered:

- `ActivitiesApi.list()`, `AutomationApi.list()`, `IntegrationsApi.list()`, `ProjectsApi.list()`,
  `ReportingApi.list()` and `SearchApi.listDocuments()` take no argument, because their specs
  declare no query parameters at all.
- `SearchApi.search(query)` is the one call whose query is **required**: `/api/v1/search` answers
  400 without `q` rather than returning everything.
- `AuditApi` has only `list`/`ingest` because the audit log is append-only; `ProjectsApi` has no
  get-by-id for milestones or deliverables, no update for links, and no direct write for emails,
  because projects-service declares none.
- `ProjectsApi` sub-resources are **created under their parent but addressed by their own id**
  afterwards: `createMilestone(projectId, …)` then `updateMilestone(milestoneId, …)`.
- `SpendApi.syncGcp()` and its three siblings are POSTs with no request body, and a 403 from
  `SpendApi.update`/`delete` is a record-source guard (only `source: "manual"` is editable), not a
  role check.
- `ReportingApi.export()` returns `SavedReport[]` **or CSV text**, selected by its `format` query
  parameter rather than by `Accept`, so its return type is the union of both representations.

## Install, generate, build, test

From `sdks/typescript-sdk/`:

```sh
npm install        # dev dependencies (typescript, @types/node)
npm run generate   # regenerate src/generated/*.ts from ../../<service>-service/openapi.yaml
npm run build      # tsc, strict, ES2022, declarations to dist/
npm test           # build + node:test unit suite
```

`npm run generate` shells out to a pinned `npx --yes openapi-typescript@6` per spec and is idempotent: rerunning it without spec changes produces byte-identical output. Generated files carry a header saying so; edit the source spec and regenerate, never the output.

## Usage

```ts
import { AccountsApi, ApiError, InfraPortalClient } from "@rodmen07/infraportal-sdk";

const client = new InfraPortalClient({
  baseUrl: "http://localhost:8080",  // gateway, or a service directly for local dev
  token: process.env.INFRAPORTAL_TOKEN, // bearer JWT; CRM routes need the admin role
});

const accounts = new AccountsApi(client);

try {
  const { data, rateLimit } = await accounts.list({ status: "active", limit: 20 });
  console.log(data.total, "accounts;", rateLimit.remaining, "requests left this second");

  const created = await accounts.create({ name: "Globex Corporation", domain: "globex.com" });
  await accounts.update(created.data.id, { status: "inactive" });
  await accounts.delete(created.data.id);
} catch (error) {
  if (error instanceof ApiError) {
    // Branch on error.code only; message wording is not part of the contract.
    if (error.code === "FORBIDDEN") {
      // Valid token, but the roles claim lacks "admin" (docs/API.md).
    }
    console.error(error.status, error.code, error.message, error.details);
  } else {
    throw error; // network error after retries, or an abort
  }
}
```

## Retry contract

The client implements the platform retry rules from `docs/RATE_LIMITING.md`:

1. On 429, honor `Retry-After` when present (integer delay-seconds; the gateway never sends HTTP-dates). Unparseable values fall through to backoff.
2. Otherwise sleep a capped full-jitter exponential backoff: `random() * min(8000, 500 * 2^attempt)` milliseconds.
3. Give up after a bounded budget (default: 5 retries after the initial attempt, so at most 6 requests) and surface the final failure.
4. Retry only 429 responses and network errors. Other statuses (401, 403, 404, 4xx, 5xx) throw `ApiError` immediately.
5. Idempotency: a 429 is generated by the gateway before proxying, so retrying it never double-applies a write; 429s are therefore retried for every verb. Network errors are retried only for idempotent verbs (GET, HEAD, OPTIONS, PUT, DELETE); POST and PATCH require an explicit per-request `idempotent: true` opt-in.
6. Aborts (`AbortError`/`TimeoutError` from a passed `signal`) are surfaced immediately and never retried.

Every success response exposes `rateLimit` (`limit`/`remaining`/`reset` parsed from the `X-RateLimit-*` headers), and every `ApiError` carries `rateLimit` plus `retryAfterSeconds`. All fields are `null` when the headers are absent, which the platform documents as normal (gateway bypass, Redis limiter fail-open). Watch `remaining` and slow down proactively as it approaches 0.

Errors follow the platform `ApiError` envelope (`{ code, message, details? }`). Only `code` is machine-readable; framework-generated text/plain rejections (400/422 from axum extractors) surface with `code: "UNKNOWN"` and the raw body as the message.

## Retry tuning and testing hooks

`InfraPortalClient` accepts optional `retry` overrides (`maxRetries`, `baseDelayMs`, `maxDelayMs`) plus injectable `sleep`, `random`, and `fetch`, which is how the unit tests drive the retry logic deterministically with no network access and no real timers.
