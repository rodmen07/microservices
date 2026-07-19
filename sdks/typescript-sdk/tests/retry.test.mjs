import test from "node:test";
import assert from "node:assert/strict";
import { ApiError, InfraPortalClient } from "../dist/index.js";
import { fakeFetch, fakeSleep, jsonResponse, rateLimited429 } from "./helpers.mjs";

const BASE = "https://gateway.example.test";

function makeClient(fetchImpl, sleep, extra = {}) {
  return new InfraPortalClient({
    baseUrl: BASE,
    token: "test-token",
    fetch: fetchImpl,
    sleep,
    ...extra,
  });
}

test("429 with Retry-After sleeps exactly that many seconds, then retries", async () => {
  const { impl, calls } = fakeFetch([
    { response: () => rateLimited429({ "Retry-After": "3" }) },
    { response: () => jsonResponse(200, { ok: true }) },
  ]);
  const { sleep, sleeps } = fakeSleep();
  // random would produce a different delay; it must not be consulted.
  const client = makeClient(impl, sleep, { random: () => 1 });

  const res = await client.request("GET", "/api/v1/accounts");
  assert.equal(res.status, 200);
  assert.equal(calls.length, 2);
  assert.deepEqual(sleeps, [3000]);
});

test("429 without Retry-After falls back to full-jitter backoff, doubling and capped", async () => {
  // 7 retries so the exponential curve passes the 8000 ms cap.
  const { impl, calls } = fakeFetch([
    { response: () => rateLimited429() },
    { response: () => rateLimited429() },
    { response: () => rateLimited429() },
    { response: () => rateLimited429() },
    { response: () => rateLimited429() },
    { response: () => rateLimited429() },
    { response: () => rateLimited429() },
    { response: () => jsonResponse(200, { ok: true }) },
  ]);
  const { sleep, sleeps } = fakeSleep();
  // random() => 1 makes the jittered delay equal the backoff ceiling.
  const client = makeClient(impl, sleep, {
    random: () => 1,
    retry: { maxRetries: 7 },
  });

  const res = await client.request("GET", "/api/v1/accounts");
  assert.equal(res.status, 200);
  assert.equal(calls.length, 8);
  assert.deepEqual(sleeps, [500, 1000, 2000, 4000, 8000, 8000, 8000]);
});

test("full jitter multiplies the backoff ceiling by random()", async () => {
  const { impl } = fakeFetch([
    { response: () => rateLimited429() },
    { response: () => jsonResponse(200, { ok: true }) },
  ]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep, { random: () => 0.5 });

  await client.request("GET", "/api/v1/accounts");
  assert.deepEqual(sleeps, [250]); // 0.5 * min(8000, 500 * 2^0)
});

test("unparseable Retry-After falls back to jittered backoff", async () => {
  const { impl } = fakeFetch([
    { response: () => rateLimited429({ "Retry-After": "soon" }) },
    { response: () => rateLimited429({ "Retry-After": "0" }) },
    { response: () => jsonResponse(200, { ok: true }) },
  ]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep, { random: () => 1 });

  await client.request("GET", "/api/v1/accounts");
  assert.deepEqual(sleeps, [500, 1000]);
});

test("gives up after the retry budget and surfaces the final 429 as ApiError", async () => {
  const steps = [];
  for (let i = 0; i < 6; i++) {
    steps.push({
      response: () =>
        rateLimited429({
          "Retry-After": "1",
          "X-RateLimit-Limit": "30",
          "X-RateLimit-Remaining": "0",
          "X-RateLimit-Reset": "1784822400",
        }),
    });
  }
  const { impl, calls } = fakeFetch(steps);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep);

  await assert.rejects(
    client.request("GET", "/api/v1/accounts"),
    (error) => {
      assert.ok(error instanceof ApiError);
      assert.equal(error.status, 429);
      assert.equal(error.code, "RATE_LIMITED");
      assert.equal(error.retryAfterSeconds, 1);
      assert.deepEqual(error.rateLimit, { limit: 30, remaining: 0, reset: 1784822400 });
      return true;
    },
  );
  // Initial attempt + 5 retries (the default budget), then give up.
  assert.equal(calls.length, 6);
  assert.equal(sleeps.length, 5);
});

test("network errors are retried for idempotent verbs", async () => {
  const { impl, calls } = fakeFetch([
    { error: new TypeError("fetch failed") },
    { error: new TypeError("fetch failed") },
    { response: () => jsonResponse(200, { ok: true }) },
  ]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep, { random: () => 1 });

  const res = await client.request("GET", "/api/v1/accounts");
  assert.equal(res.status, 200);
  assert.equal(calls.length, 3);
  assert.deepEqual(sleeps, [500, 1000]);
});

test("network errors exhaust the budget and rethrow the last error", async () => {
  const steps = [];
  for (let i = 0; i < 6; i++) steps.push({ error: new TypeError("fetch failed") });
  const { impl, calls } = fakeFetch(steps);
  const { sleep } = fakeSleep();
  const client = makeClient(impl, sleep, { random: () => 1 });

  await assert.rejects(client.request("GET", "/api/v1/accounts"), TypeError);
  assert.equal(calls.length, 6);
});

test("network error on POST is not retried by default", async () => {
  const { impl, calls } = fakeFetch([{ error: new TypeError("fetch failed") }]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep);

  await assert.rejects(
    client.request("POST", "/api/v1/accounts", { body: { name: "Globex" } }),
    TypeError,
  );
  assert.equal(calls.length, 1);
  assert.equal(sleeps.length, 0);
});

test("network error on POST is retried when the caller opts in with idempotent: true", async () => {
  const { impl, calls } = fakeFetch([
    { error: new TypeError("fetch failed") },
    { response: () => jsonResponse(201, { id: "a1" }) },
  ]);
  const { sleep } = fakeSleep();
  const client = makeClient(impl, sleep, { random: () => 1 });

  const res = await client.request("POST", "/api/v1/accounts", {
    body: { name: "Globex" },
    idempotent: true,
  });
  assert.equal(res.status, 201);
  assert.equal(calls.length, 2);
});

test("429 on POST is retried automatically (gateway rejects before proxying)", async () => {
  const { impl, calls } = fakeFetch([
    { response: () => rateLimited429({ "Retry-After": "1" }) },
    { response: () => jsonResponse(201, { id: "a1" }) },
  ]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep);

  const res = await client.request("POST", "/api/v1/accounts", {
    body: { name: "Globex" },
  });
  assert.equal(res.status, 201);
  assert.equal(calls.length, 2);
  assert.deepEqual(sleeps, [1000]);
});

test("non-429 HTTP errors are never retried", async () => {
  const { impl, calls } = fakeFetch([
    { response: () => jsonResponse(500, { code: "DB_ERROR", message: "database error" }) },
  ]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep);

  await assert.rejects(client.request("GET", "/api/v1/accounts"), (error) => {
    assert.ok(error instanceof ApiError);
    assert.equal(error.status, 500);
    assert.equal(error.code, "DB_ERROR");
    return true;
  });
  assert.equal(calls.length, 1);
  assert.equal(sleeps.length, 0);
});

test("aborts are surfaced immediately and never retried", async () => {
  const abortError = new Error("This operation was aborted");
  abortError.name = "AbortError";
  const { impl, calls } = fakeFetch([{ error: abortError }]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep);

  await assert.rejects(client.request("GET", "/api/v1/accounts"), (error) => {
    assert.equal(error.name, "AbortError");
    return true;
  });
  assert.equal(calls.length, 1);
  assert.equal(sleeps.length, 0);
});

test("no sleep when the first attempt succeeds", async () => {
  const { impl } = fakeFetch([{ response: () => jsonResponse(200, { ok: true }) }]);
  const { sleep, sleeps } = fakeSleep();
  const client = makeClient(impl, sleep);

  await client.request("GET", "/api/v1/accounts");
  assert.equal(sleeps.length, 0);
});
