# Platform Rate Limiting Guide

Adoption guide for the InfraPortal platform rate limits. Created 2026-07-18 as part of v1.16.2 (Developer Experience). Companion to the [platform API guide](API.md) and the per-service OpenAPI specs.

> **Runtime status:** all runtime endpoints have been offline since 2026-06-04, when the platform infrastructure was decommissioned to true zero (see `ROADMAP.md`). This guide documents the rate-limiting contract **as implemented in go-gateway** (`internal/middleware/ratelimit.go` and `ratelimit_redis.go`, shipped in v1.10), verified by that repo's tests, not against live endpoints.

---

## Where limits are enforced

Rate limiting lives entirely in **go-gateway**. The Rust and Python services perform no rate limiting of their own.

- Every request proxied through the gateway is checked against a per-client, per-route token bucket and receives `X-RateLimit-*` headers on the response, success or error.
- Requests sent **directly to a service** (bypassing the gateway, for example in local development) are never rate limited and carry none of these headers. The per-service OpenAPI specs (for example [`accounts-service/openapi.yaml`](../accounts-service/openapi.yaml)) document the headers with this same caveat.
- The gateway's own local endpoints (`/health`, `/health/upstreams`, `/api/openapi.json`, `/api/docs`) are registered outside the middleware chain, so they are not rate limited and carry no headers either.

---

## Tiers

Limits are assigned by **route prefix**, not by HTTP method. Each proxied route prefix belongs to a tier; any route not matched by a tier prefix falls back to the default tier. All values are per client IP, per route, and configurable by environment variable (defaults shown, from `go-gateway/internal/config/config.go`):

| Tier | Limit | Env override | Route prefixes |
|------|-------|--------------|----------------|
| auth | 5 rps | `RATE_LIMIT_AUTH_RPS` | `/api/auth` |
| write | 30 rps | `RATE_LIMIT_WRITE_RPS` | `/api/accounts`, `/api/contacts`, `/api/opportunities`, `/api/activities`, `/api/automation`, `/api/integrations`, `/api/tasks`, `/api/v1/projects` |
| read | 60 rps | `RATE_LIMIT_READ_RPS` | `/api/reporting`, `/api/search`, `/api/events` |
| default | 15 rps | `RATE_LIMIT_RPS` | any other proxied route |

Notes on how classification behaves in practice:

- The write tier covers the write-capable CRM route groups for **all** methods on those prefixes. A `GET /api/accounts` counts against the 30 rps write tier, because the accounts routes are write-capable; the read tier is for the read-only route groups (reporting, search, events).
- The auth tier is deliberately the tightest, to slow credential-stuffing attempts against token issuance.
- **Burst allowance (in-memory limiter):** each bucket holds up to 2x its per-second rate, so short bursts of 10 (auth), 60 (write), 120 (read), or 30 (default) requests are absorbed before throttling begins. A bucket idle for more than 5 minutes is evicted and starts full again on next use.

### Bucket keying

The bucket key is `client IP + route key`, where the route key is the first two path segments of the request (`/api/accounts/123` keys as `/api/accounts`). One consequence: everything under `/api/v1/` shares the `/api/v1` bucket, which today only holds the projects routes. The client IP is taken from the **rightmost** `X-Forwarded-For` entry (the hop appended by the trusted load balancer), falling back to the TCP remote address, so callers cannot rotate a spoofed leftmost entry to evade limits.

---

## Response headers

Every gateway-proxied response, including 4xx and 5xx responses, carries:

| Header | Meaning |
|--------|---------|
| `X-RateLimit-Limit` | Integer requests per second allowed for the matched tier. |
| `X-RateLimit-Remaining` | Whole tokens left in this client's bucket, floored at 0. Measured immediately before the current request consumes its token, so treat it as advisory: values of 0 or 1 mean you are at the edge of the limit. |
| `X-RateLimit-Reset` | Unix epoch second at which capacity is next available. Because the token bucket refills continuously, the implementation always reports one second after the response time; treat it as "the earliest second to try again", not a window boundary. |

---

## The 429 response

When the bucket is empty the gateway rejects the request itself; it is **never forwarded upstream**. The response is:

- **Status:** `429 Too Many Requests`
- **Headers:** `Retry-After` plus the three `X-RateLimit-*` headers (`X-RateLimit-Remaining` will be 0)
- **Content-Type:** `application/json`
- **Body** (the platform `ApiError` envelope with no `details` field):

```json
{
  "code": "RATE_LIMITED",
  "message": "rate limit exceeded (30 req/s), retry after 1s"
}
```

Parse only `code`. The `message` text embeds the limit and retry delay for humans, and its exact punctuation differs between the in-memory and Redis limiter implementations, so never match on it.

### Retry-After semantics

`Retry-After` is always an **integer number of seconds** (delay-seconds form), never an HTTP-date. The implementation computes it as `floor(1 / rps) + 1`, which evaluates to `1` for every shipped tier (any tier of 1 rps or more). Parse the header rather than hardcoding 1: the value grows if a tier is ever configured below 1 rps, and correct clients should not depend on the current constants.

---

## Handling 429s correctly

The rules, in priority order:

1. **Respect `Retry-After` when present.** It is the server telling you exactly how long to wait. Sleeping less just burns your budget on guaranteed rejections.
2. **Otherwise use capped exponential backoff with jitter.** Start around 500 ms, double per attempt, cap around 8 s, and use full jitter (a random delay between 0 and the computed backoff) so many clients do not retry in lockstep.
3. **Bound the retries.** Give up after a small fixed budget (5 attempts in the snippets below) and surface the 429 to the caller.
4. **Slow down proactively.** When `X-RateLimit-Remaining` approaches 0, spread out your requests instead of waiting to be rejected.

### Reference sequence

A typical throttled exchange, in prose:

1. Client sends `POST /api/accounts` to the gateway.
2. The bucket for (client IP, `/api/accounts`) is empty. The gateway responds `429` with `Retry-After: 1`, `X-RateLimit-Limit: 30`, `X-RateLimit-Remaining: 0`, and the `RATE_LIMITED` body. The request never reached accounts-service.
3. Client reads `Retry-After` and sleeps 1 second.
4. Client resends the identical `POST /api/accounts`.
5. The bucket has refilled. The gateway forwards the request to accounts-service and relays its `201 Created`, with fresh `X-RateLimit-*` headers attached.
6. If step 4 had produced another 429 with no parseable `Retry-After`, the client would fall back to exponential backoff with jitter, and stop after its retry budget.

### Idempotency cautions for retried writes

- A 429 is generated by the gateway **before proxying**, so the upstream service never saw the request. Retrying after a pure 429 can never double-apply a write.
- The danger is retry helpers that also retry timeouts and 5xx responses: there a non-idempotent `POST` may already have been applied upstream. For writes, either keep automatic retries scoped to 429 (as the snippets below do), or make the write naturally idempotent (client-generated IDs, check-then-create).
- `PUT` and `DELETE` on a specific resource are safe to retry more broadly; unqualified `POST` creates are not.

---

## Retry snippets

Each snippet honors `Retry-After` first, falls back to capped exponential backoff with full jitter, retries only on 429, and gives up after 5 retries.

### TypeScript (fetch)

```typescript
async function fetchWithRetry(
  input: RequestInfo | URL,
  init: RequestInit = {},
  maxRetries = 5,
): Promise<Response> {
  const baseMs = 500;
  const capMs = 8000;
  for (let attempt = 0; ; attempt++) {
    const res = await fetch(input, init);
    if (res.status !== 429 || attempt >= maxRetries) return res;
    const retryAfter = Number(res.headers.get("Retry-After"));
    const delayMs =
      Number.isFinite(retryAfter) && retryAfter > 0
        ? retryAfter * 1000
        : Math.random() * Math.min(capMs, baseMs * 2 ** attempt); // full jitter
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
}
```

### Python (requests)

```python
import random
import time

import requests

BASE_DELAY = 0.5
MAX_DELAY = 8.0
MAX_RETRIES = 5


def request_with_retry(method: str, url: str, **kwargs) -> requests.Response:
    session = kwargs.pop("session", None) or requests.Session()
    for attempt in range(MAX_RETRIES + 1):
        resp = session.request(method, url, **kwargs)
        if resp.status_code != 429 or attempt == MAX_RETRIES:
            return resp
        retry_after = resp.headers.get("Retry-After", "")
        if retry_after.isdigit() and int(retry_after) > 0:
            delay = float(retry_after)
        else:
            delay = random.uniform(0.0, min(MAX_DELAY, BASE_DELAY * 2**attempt))
        time.sleep(delay)
    return resp
```

### Go (net/http)

```go
package apiclient

import (
	"math/rand"
	"net/http"
	"strconv"
	"time"
)

// DoWithRetry sends req, honoring Retry-After on 429 and otherwise using
// capped exponential backoff with full jitter. For requests with a body,
// set req.GetBody so the body can be replayed on retry.
func DoWithRetry(c *http.Client, req *http.Request) (*http.Response, error) {
	const maxRetries = 5
	base, ceiling := 500*time.Millisecond, 8*time.Second
	for attempt := 0; ; attempt++ {
		if req.GetBody != nil {
			body, err := req.GetBody()
			if err != nil {
				return nil, err
			}
			req.Body = body
		}
		resp, err := c.Do(req)
		if err != nil || resp.StatusCode != http.StatusTooManyRequests || attempt >= maxRetries {
			return resp, err
		}
		resp.Body.Close()
		var delay time.Duration
		if secs, convErr := strconv.Atoi(resp.Header.Get("Retry-After")); convErr == nil && secs > 0 {
			delay = time.Duration(secs) * time.Second
		} else {
			delay = time.Duration(rand.Int63n(int64(min(ceiling, base<<attempt)) + 1))
		}
		time.Sleep(delay)
	}
}
```

---

## Distributed limiter (Redis, staged rollout)

Setting `ENABLE_REDIS_RATE_LIMITER=true` with a `REDIS_URL` swaps the in-memory token bucket for a Redis-backed limiter (`internal/middleware/ratelimit_redis.go`) whose state is shared across all gateway instances. The client-facing contract is the same (same headers, same 429 body shape, same `RATE_LIMITED` code), with these behavioral differences:

- It is a **fixed one-second window** (`INCR` on a `rl:<ip>:<route>:<second>` key with a 2 second expiry), so there is no 2x burst allowance: exactly the tier's rps is admitted per second.
- `Retry-After` is always `1`.
- It **fails open**: if Redis is unreachable the request is proxied without limiting, and in that path no `X-RateLimit-*` headers are set. Well-behaved clients should therefore tolerate the headers being absent.

The toggle defaults to false; the in-memory limiter is the shipped default.

---

## See also

- [`docs/API.md`](API.md), the platform getting-started guide (authentication, error envelope, service index).
- [`accounts-service/openapi.yaml`](../accounts-service/openapi.yaml), which documents the `X-RateLimit-*` headers and the `RateLimited` response as reusable components; the remaining per-service specs are indexed in [API.md](API.md#per-service-api-specs) and land in v1.16.1 PR2.
- `ROADMAP.md` in the repo root for v1.10 (gateway rate limiting) and v1.16.2 (this guide).
