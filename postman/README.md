# InfraPortal Postman Collection

Ready-to-import Postman artifacts for the InfraPortal CRM platform API, generated from the eleven per-service OpenAPI 3.0.3 specs in this repo (v1.16.3 PR2). Companion to the [platform API guide](../docs/API.md) and the [rate limiting guide](../docs/RATE_LIMITING.md).

> **Runtime status:** all runtime endpoints have been offline since 2026-06-04, when the platform infrastructure was decommissioned to true zero (see `ROADMAP.md`). The collection documents the API contract **as implemented in code**; use it against a locally running stack, not the historical cloud URLs.

## Files

| File | Purpose |
|------|---------|
| `infraportal.postman_collection.json` | Postman Collection v2.1. One folder per service (11 folders, 99 requests), collection-level bearer auth reading `{{token}}`, every request URL rooted at `{{baseUrl}}`. Generated; do not edit by hand. |
| `infraportal.postman_environment.json` | Environment template: `baseUrl` (default `http://localhost:8080`, the local go-gateway) and `token` (empty, secret type). |
| `generate-collection.mjs` | Regenerates the collection from the specs. |

## Import

1. In Postman: **Import**, then drop in both `infraportal.postman_collection.json` and `infraportal.postman_environment.json` (or select them via **files**).
2. Select the **InfraPortal Local** environment in the environment picker.
3. Paste a JWT into the `token` environment variable (see below). All requests inherit the collection-level bearer auth; the unauthenticated `/health` and `/ready` probes simply ignore the header.
4. Adjust `baseUrl` if needed: the default targets the local go-gateway on port 8080. To call a service directly (bypassing the gateway; no rate limiting, no `X-RateLimit-*` headers), point `baseUrl` at the service port instead, for example `http://localhost:3010` for accounts-service via `docker compose up`.

## Minting a local dev token

There is no live token issuer. For local development, sign an HS256 JWT with the services' default dev secret (`AUTH_JWT_SECRET` defaults to `dev-insecure-secret-change-me`), issuer `auth-service`, and the `admin` role, which every CRM `/api/v1` route requires. Claim semantics and the 401/403 matrix are documented in [docs/API.md](../docs/API.md).

No-dependency Node one-liner (paste the output into the `token` variable):

```bash
node -e "
const crypto = require('crypto');
const b64 = (o) => Buffer.from(JSON.stringify(o)).toString('base64url');
const now = Math.floor(Date.now() / 1000);
const header = b64({ alg: 'HS256', typ: 'JWT' });
const payload = b64({ sub: 'postman-dev', iss: 'auth-service', roles: ['admin'], iat: now, exp: now + 3600 });
const sig = crypto.createHmac('sha256', 'dev-insecure-secret-change-me').update(header + '.' + payload).digest('base64url');
console.log(header + '.' + payload + '.' + sig);
"
```

The secret is deliberately insecure and for local development only; never reuse it in a deployed environment.

## Regenerating

```bash
node postman/generate-collection.mjs
```

Requires Node 18+ and network access on first run (it invokes the converter via a pinned `npx --yes -p openapi-to-postmanv2@4 openapi2postmanv2` per spec, then merges the results). The script fails loudly if any of the eleven `<service>-service/openapi.yaml` specs is missing, if a conversion fails, or if any request is not rooted at `{{baseUrl}}`.

Output is deterministic: volatile converter ids are stripped, the converter's schema faker is pinned to a fixed-seed `Math.random` (its enum picks are otherwise random per run), and keys are serialized sorted, so rerunning over unchanged specs is byte-identical and the committed artifact stays diff-stable.
