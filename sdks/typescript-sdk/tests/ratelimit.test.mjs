import test from "node:test";
import assert from "node:assert/strict";
import {
  ApiError,
  InfraPortalClient,
  parseRateLimit,
  parseRetryAfterSeconds,
} from "../dist/index.js";
import { fakeFetch, fakeSleep, jsonResponse } from "./helpers.mjs";

const BASE = "https://gateway.example.test";

function makeClient(fetchImpl) {
  const { sleep } = fakeSleep();
  return new InfraPortalClient({ baseUrl: BASE, token: "t", fetch: fetchImpl, sleep });
}

test("parseRateLimit extracts the three X-RateLimit-* headers as numbers", () => {
  const headers = new Headers({
    "X-RateLimit-Limit": "30",
    "X-RateLimit-Remaining": "29",
    "X-RateLimit-Reset": "1784822400",
  });
  assert.deepEqual(parseRateLimit(headers), { limit: 30, remaining: 29, reset: 1784822400 });
});

test("parseRateLimit yields nulls when headers are absent (gateway bypass or Redis fail-open)", () => {
  assert.deepEqual(parseRateLimit(new Headers()), { limit: null, remaining: null, reset: null });
});

test("parseRateLimit yields null for non-numeric or blank values", () => {
  const headers = new Headers({
    "X-RateLimit-Limit": "lots",
    "X-RateLimit-Remaining": " ",
    "X-RateLimit-Reset": "1784822400",
  });
  assert.deepEqual(parseRateLimit(headers), { limit: null, remaining: null, reset: 1784822400 });
});

test("successful responses expose parsed rate-limit info", async () => {
  const { impl } = fakeFetch([
    {
      response: () =>
        jsonResponse(200, { data: [], total: 0, limit: 50, offset: 0 }, {
          "X-RateLimit-Limit": "60",
          "X-RateLimit-Remaining": "59",
          "X-RateLimit-Reset": "1784822400",
        }),
    },
  ]);
  const client = makeClient(impl);

  const res = await client.request("GET", "/api/v1/accounts");
  assert.deepEqual(res.rateLimit, { limit: 60, remaining: 59, reset: 1784822400 });
});

test("error responses carry parsed rate-limit info on the ApiError", async () => {
  const { impl } = fakeFetch([
    {
      response: () =>
        jsonResponse(403, { code: "FORBIDDEN", message: "admin role required" }, {
          "X-RateLimit-Limit": "30",
          "X-RateLimit-Remaining": "12",
          "X-RateLimit-Reset": "1784822401",
        }),
    },
  ]);
  const client = makeClient(impl);

  await assert.rejects(client.request("GET", "/api/v1/accounts"), (error) => {
    assert.ok(error instanceof ApiError);
    assert.deepEqual(error.rateLimit, { limit: 30, remaining: 12, reset: 1784822401 });
    return true;
  });
});

test("parseRetryAfterSeconds parses delay-seconds and rejects everything else", () => {
  assert.equal(parseRetryAfterSeconds(new Headers({ "Retry-After": "1" })), 1);
  assert.equal(parseRetryAfterSeconds(new Headers({ "Retry-After": "7" })), 7);
  assert.equal(parseRetryAfterSeconds(new Headers()), null);
  assert.equal(parseRetryAfterSeconds(new Headers({ "Retry-After": "0" })), null);
  assert.equal(parseRetryAfterSeconds(new Headers({ "Retry-After": "-2" })), null);
  // The gateway never sends HTTP-dates; they must not parse.
  assert.equal(
    parseRetryAfterSeconds(new Headers({ "Retry-After": "Wed, 21 Oct 2026 07:28:00 GMT" })),
    null,
  );
});
