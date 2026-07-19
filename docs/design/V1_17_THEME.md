# v1.17 Theme Proposal: Interactive API Playground

**Status: PROPOSED - awaiting user review.** This is a design document, not scheduled work. The autonomous workflow must not pick up any v1.17.x item until the user approves the theme and resolves the decisions in the final section.

- Created: 2026-07-19, branch `autodev/v1.17-theme-proposal`
- Canonical roadmap: [`ROADMAP.md`](../../ROADMAP.md) (decommission status, doable-now / BLOCKED / USER-ONLY partition)
- Frontend roadmap: `d:/Projects/Portfolio/infraportal/ROADMAP.md`

---

## Constraint recap and selection criteria

All runtime infrastructure was decommissioned to true zero on 2026-06-04. Nothing deploys except the infraportal frontend (GitHub Pages on merge to main), and the verification bar for platform work is build + tests + CI, never a live endpoint. The infra-rebuild decision is USER-ONLY and has not been made.

The portfolio exists for the job and contract hunt. Its audience is two people:

1. **The visitor**: a prospective employer or client who opens the deployed site and spends minutes, not hours. They judge what they can SEE.
2. **The reviewer**: an engineer who opens the repos. They judge what they can VERIFY: tests, CI, honest boundaries, code quality.

A v1.17 theme is therefore scored on:

- **SEE**: how much of it lands on the one surface that still deploys.
- **VERIFY**: how much of it is provable in CI by a skeptical reviewer.
- **Compounding**: how much it reuses what v1.16 already paid for.
- **Cadence fit**: whether it splits into weekly minors of one or two small PRs each.
- **Honesty**: no fake liveness. Every demo behavior must be labeled as in-browser demo data, following the mock-boundary pattern already established in the frontend.

## What v1.16 delivered (the inputs v1.17 can build on)

- OpenAPI 3.0.3 specs for all 11 workspace services (`<service>/openapi.yaml`), linted in CI by `.github/workflows/openapi-validation.yml` (Redocly CLI pinned to 1.25.15).
- [`docs/API.md`](../API.md) (auth model, 401/403 semantics, error envelope, rate-limit tiers, spec index) and [`docs/RATE_LIMITING.md`](../RATE_LIMITING.md).
- go-gateway `/api/openapi.json` spec aggregator route with unit tests (separate repo, offline).
- TypeScript SDK at `sdks/typescript-sdk/`: `InfraPortalClient` core (retry contract, rate-limit parsing, error envelope) plus generated types for all 11 services under `src/generated/`, but only **one** typed service module (`src/services/accounts.ts`). Ten remain. Build-only; npm publish is USER-ONLY.
- Postman collection in `postman/`.
- Frontend (infraportal repo, deployed): bulk import/edit and templates/cloning on the in-browser demo stores (`crmStore.mock.ts`, `projectsStore.mock.ts`, `bulkImportApi.mock.ts`, `bulkEditApi.mock.ts`), each behind an explicit MOCK DATA BOUNDARY marker.

One thread is left dangling, and it matters: the deployed site has an API docs page at `#/api-docs` (`src/pages/ApiDocsPage.tsx`), but its nav link was removed in the 2026-06-26 monetization pivot, its "Open Swagger UI" and "Download OpenAPI Spec" buttons point at the dead gateway URL, and its service list is hardcoded copy that is not derived from the committed specs. The platform's single biggest v1.16 artifact, the specs, is invisible to the visitor.

Still BLOCKED and untouched by this proposal: Cost Intelligence, client email notifications, anything needing live infrastructure (see `ROADMAP.md`).

---

## Candidate themes

Four candidates were evaluated seriously, plus one rejected early.

### A. Interactive API Playground (recommended)

Wire the committed OpenAPI specs into the deployed site: a client-side API reference rendering all 11 specs, a request builder that executes against the in-browser demo stores where they exist, generated curl and SDK snippets, and the API Docs nav link restored.

- SEE: **High.** The largest v1.16 artifact becomes the most visible page on the only deployed surface. A visitor can browse every endpoint and actually execute requests against labeled demo data.
- VERIFY: **High.** Spec-to-UI rendering, the operation-to-store adapter, and snippet generation are all unit-testable in Vitest; the spec snapshot gets a drift check in CI.
- Compounding: **Best of the four.** Consumes the specs (v1.16.1), the error envelope and rate-limit docs (v1.16.2), the SDK surface (v1.16.3), and the demo stores (v1.16.4/.5). Also closes the already-tracked follow-up in both roadmaps (restore nav link, point the page at real specs).
- Cadence fit: splits cleanly into four one-or-two-PR weekly slices (below).
- Risks: renderer scope creep, bundle size, cross-repo spec drift, demo-data coverage gaps. All mitigable (see Risks).

### B. SDK completion and showcase

Write the ten remaining typed service modules (activities, audit, automation, contacts, integrations, opportunities, projects, reporting, search, spend) following the `AccountsApi` pattern, plus a live SDK-powered demo page on the site.

- SEE: **Medium at best.** The SDK is unpublished (npm publish is USER-ONLY), so a visitor cannot install it; the demo page would need a fetch-level mock adapter and would largely duplicate what the playground's request builder shows more directly.
- VERIFY: **High.** tsc plus unit tests against a mocked fetch; mechanical, low-risk work.
- Honest assessment: this is a completion, not a theme. Ten modules stamped from one proven pattern is a week or two of low-signal repetition, and its showcase half is a worse version of candidate A. Verdict: **defer.** Strong v1.18 candidate, much stronger if the user publishes the SDK to npm first. Candidate A absorbs the visible part (per-operation SDK snippets using the real published-in-source API).

### C. Quality theater to quality reality

Mutation testing (cargo-mutants), coverage gates (cargo-llvm-cov), cargo-semver-checks across the workspace, and a public quality dashboard page fed by CI-committed artifacts.

- SEE: **Low without major extra work.** The dashboard page is the only visible part, and it is a rendering of numbers whose meaning most visitors will not evaluate.
- VERIFY: **Highest of the four.** This is exactly what a senior reviewer respects.
- Honest assessment: the practical friction is real. Mutation testing across 11 crates is heavy CI runtime with a long tuning loop, and `cargo` is not available in the autodev Bash shell, so every tuning iteration round-trips through CI. High risk of burning two of the four weeks on runtime budgets instead of shipping. Verdict: **defer as a theme.** Adopt the cheapest piece (cargo-semver-checks as a single CI job) as a standalone quick win any week it fits; keep the full theme as a v1.18 candidate.

### D. Case-study content engine

Auto-generate architecture and case-study pages on the site from this repo's own docs (roadmap, API guide, chaos runbook, patch notes).

- SEE: **High volume, low weight.** More pages, but generated prose reads as filler to exactly the audience this portfolio targets.
- VERIFY: **Low.** "The generator ran" is not a quality signal; correctness of generated claims is fuzzy and untestable in any meaningful way.
- Honest assessment: the site just went through a truth remediation pass (2026-07-18) removing dead-backend claims. An engine that auto-generates claims from docs reintroduces truth-drift risk in the opposite direction. Verdict: **reject as a theme.** Individual hand-written case studies remain fine as one-off candidates.

### E. Platform in the browser (rejected early)

Compile or reimplement service logic to run fully client-side (WASM, sql.js) so the "whole platform" runs in the visitor's tab. Maximum wow, but the axum + sqlx + PostgreSQL services do not compile to WASM without a rewrite, so it would demo a reimplementation while implying it is the real code, which fails the honesty bar. Oversized for a weekly cadence. Rejected.

### Comparison summary

| Criterion | A. Playground | B. SDK completion | C. Quality reality | D. Content engine |
|---|---|---|---|---|
| SEE (deployed site) | High | Medium | Low | Medium |
| VERIFY (CI, tests) | High | High | Highest | Low |
| Reuses v1.16 outputs | Highest | Medium | Low | Low |
| Weekly-slice fit | Good | Good | Poor (CI tuning loop) | Fair |
| Honesty risk | Low (labeled mocks) | Low | Low | High (generated claims) |
| Verdict | **Recommended** | Defer (v1.18) | Defer (v1.18) | Reject |

---

## Recommended theme: Interactive API Playground

One sentence: **make the API surface of the platform something a visitor can browse, execute, and copy, entirely client-side, entirely honest about being demo data.**

Repo split follows the v1.16.4/.5 precedent: the theme is versioned on this platform roadmap, but most code lands in the infraportal repo (the only deployable surface). Microservices-repo work is limited to docs cross-links and keeping the specs authoritative. If approved, the infraportal `ROADMAP.md` gets the per-milestone frontend detail as part of v1.17.1 PR 1.

### Non-goals

- No live endpoints, no infra rebuild, no deploy-pipeline changes.
- No npm publish (USER-ONLY; see decision D4).
- No extension of the demo stores to all 11 services. Services without demo data get an explicit "static reference, no demo dataset" state, never a fake success.
- No completion of the ten SDK service modules (candidate B) beyond generating per-operation snippets from the one real pattern.

### Milestones

Cadence: four weekly minors, target weeks of 2026-07-20, 2026-07-27, 2026-08-03, 2026-08-10. Each slice is independently shippable; if a week slips, later slices shift rather than grow. Verification bar throughout: `npm run build`, Vitest, the tsc CI gate, and a green Pages deploy (frontend); CI green (this repo).

#### v1.17.1 - Committed specs rendered on the site

- PR 1 (infraportal): spec snapshot pipeline. An `npm run sync-specs` script reads the 11 `*/openapi.yaml` files from the sibling microservices checkout and emits committed JSON snapshots into the site (JSON at sync time, so no runtime YAML parser ships in the bundle). Rework `ApiDocsPage.tsx` to render services, operations, parameters, and schemas from the committed specs; delete the dead `GATEWAY_URL` links and the hardcoded `SERVICES` table.
- PR 2 (infraportal): restore the API Docs nav link; truth pass on the page copy so every count and claim derives from the specs; add a Vitest that walks every bundled spec and asserts every operation renders without a fallback path.
- Done when: `#/api-docs` renders all 11 specs offline with zero network requests, the nav link is visible, and the walk-every-operation test is green in CI.

#### v1.17.2 - Request builder against the demo stores

- PR 1 (infraportal): a "Try it" panel per operation with forms generated from the spec's parameters and requestBody schema, executing through an adapter that maps spec operations onto the existing demo stores: accounts, contacts, opportunities (`crmStore.mock.ts`) and projects (`projectsStore.mock.ts`). Responses use the documented error envelope; the adapter reuses the established mock-boundary marker pattern (`CRM_STORE_BOUNDARY`) and the panel labels itself as in-browser demo data. Services without demo data show the labeled static-reference state.
- PR 2 (infraportal): response viewer with status, body, and simulated headers: `VALIDATION_ERROR` and `NOT_FOUND` cases behave per the specs, and `X-RateLimit-*` headers are shown per the documented tiers with an explicit "simulated" tag. Vitest coverage of the operation-to-store mapping and the validation behavior.
- Done when: a visitor can execute list/get/create/update/delete against in-browser data for the four covered services, uncovered services never fake a success, and the adapter tests are green.

#### v1.17.3 - Snippets and deep links

- PR 1 (infraportal): per-operation copyable snippets: curl (with auth header and error-envelope notes from `docs/API.md`) and TypeScript SDK usage following the real `@rodmen07/infraportal-sdk` surface in `sdks/typescript-sdk/`, labeled "builds from source; not yet on npm". Snippet generation is template-driven and unit tested.
- PR 2 (infraportal): sharable deep links (`#/api-docs?service=accounts&op=listAccounts`) that restore the selected operation and panel state, so specific endpoints can be linked from case studies and outreach.
- Done when: snippet tests are green and a deep link cold-loads to the right operation.

#### v1.17.4 - Drift protection and wrap-up

- PR 1 (infraportal): a CI job that compares the committed spec snapshots against the specs in the public microservices repo and surfaces drift as a visible, non-blocking warning with a documented resync command. The committed snapshot stays authoritative for the site; the check exists so drift is noticed, not to break unrelated PRs.
- PR 2 (this repo): `docs/API.md` gains a "browse and try these specs on the deployed site" section linking the playground; patch notes entry; `ROADMAP.md` updated to record v1.17 as delivered.
- Done when: the drift job passes on a clean sync and both repos cross-link.

### What this proves to the audience

- **To a visitor (employer or client):** a living, navigable API reference for an 11-service platform, where requests actually execute and errors, auth semantics, and rate-limit headers behave as documented. It reads as a product, not a README.
- **To a code reviewer:** spec-driven UI generation, a tested adapter with an honest and explicit mock boundary, a drift check across repos, and restraint (no fake liveness, no invented data). The 2026-06-04 decommission is presented as a deliberate cost decision rather than hidden.
- **For the funnel:** the API Docs page becomes credible linked collateral for the consulting funnel and the unpublished LinkedIn post, with deep links to specific endpoints.

### Risks and mitigations

- **Renderer scope creep.** OpenAPI is large; a general renderer is a project in itself. Mitigation: render only what the 11 hand-written specs actually use, enforced by the walk-every-operation test; anything unsupported fails the test loudly instead of rendering wrong.
- **Bundle size.** Mitigation: JSON snapshots (no runtime YAML parser), lazy-load the specs and the playground route, and record the bundle delta in each PR description. If the custom renderer proves too costly, the fallback is adopting `swagger-ui-react` (decision D2), accepting its weight.
- **Cross-repo drift.** The specs live in this repo; the site builds from the infraportal repo. Mitigation: committed snapshots plus the v1.17.4 drift warning; sync is a one-command script.
- **Demo-data coverage gap.** Only 4 of 11 services have demo stores. Mitigation: the labeled static-reference state is a designed feature of the page, not an apology; extending demo coverage is explicitly out of scope.
- **Dependency and audit surface.** The known npm-audit findings await a coordinated Vite major upgrade (tracked in the infraportal roadmap). Mitigation: the recommended custom renderer adds no new runtime dependencies; only decision D2's fallback would.

---

## User decisions requested

- **D1. Theme approval.** Approve Interactive API Playground as the v1.17 theme, or redirect to candidate B (SDK completion) or C (Quality reality). Nothing under v1.17 is doable-now until this is decided.
- **D2. Renderer approach.** Recommended: a custom lightweight renderer scoped to what the committed specs use (smaller bundle, more reviewable code, no new dependencies). Alternative: adopt `swagger-ui-react` (faster, heavier, adds audit surface). Recommendation stands unless v1.17.1 PR 1 proves the custom path too costly.
- **D3. Nav change approval.** Restoring the API Docs nav link touches the monetization funnel's primary surface (the nav was deliberately pruned in the 2026-06-26 pivot). Confirm the link returns, and where it sits relative to the funnel entries.
- **D4. Optional, USER-ONLY: npm publish of the TypeScript SDK.** Not required by this theme. If published, v1.17.3's snippets become installable-real, and candidate B rises sharply as the v1.18 theme.
- **D5. Cadence confirmation.** Four weekly minors, v1.17.1 through v1.17.4, target weeks of 2026-07-20 through 2026-08-10, wrapped up by a v1.17 summary entry in `ROADMAP.md`.

If approved, this document is superseded by the scheduled milestones in `ROADMAP.md` and the per-milestone frontend detail in the infraportal `ROADMAP.md`; it then remains as the design record. This proposal also absorbs the "Frontend follow-up" line in both roadmaps (restore the API Docs nav link and point the page at the committed specs), which becomes v1.17.1.
