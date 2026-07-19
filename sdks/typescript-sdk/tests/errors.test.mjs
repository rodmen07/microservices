import test from "node:test";
import assert from "node:assert/strict";
import {
  ApiError,
  InfraPortalClient,
  UNKNOWN_ERROR_CODE,
  parseErrorEnvelope,
} from "../dist/index.js";
import { fakeFetch, fakeSleep, jsonResponse, textResponse } from "./helpers.mjs";

const BASE = "https://gateway.example.test";

function makeClient(fetchImpl) {
  const { sleep } = fakeSleep();
  return new InfraPortalClient({ baseUrl: BASE, token: "t", fetch: fetchImpl, sleep });
}

test("403 envelope maps to ApiError with code FORBIDDEN and no details", async () => {
  const { impl } = fakeFetch([
    { response: () => jsonResponse(403, { code: "FORBIDDEN", message: "admin role required" }) },
  ]);
  const client = makeClient(impl);

  await assert.rejects(client.request("GET", "/api/v1/accounts"), (error) => {
    assert.ok(error instanceof ApiError);
    assert.ok(error instanceof Error);
    assert.equal(error.status, 403);
    assert.equal(error.code, "FORBIDDEN");
    assert.equal(error.message, "admin role required");
    assert.equal(error.details, undefined);
    assert.equal(error.retryAfterSeconds, null);
    return true;
  });
});

test("400 envelope preserves structured details", async () => {
  const { impl } = fakeFetch([
    {
      response: () =>
        jsonResponse(400, {
          code: "VALIDATION_ERROR",
          message: "name must not be empty",
          details: { field: "name", constraint: "must not be empty" },
        }),
    },
  ]);
  const client = makeClient(impl);

  await assert.rejects(client.request("POST", "/api/v1/accounts", { body: { name: "" } }), (error) => {
    assert.ok(error instanceof ApiError);
    assert.equal(error.status, 400);
    assert.equal(error.code, "VALIDATION_ERROR");
    assert.deepEqual(error.details, { field: "name", constraint: "must not be empty" });
    return true;
  });
});

test("text/plain axum rejection maps to code UNKNOWN with the raw body as message", async () => {
  const raw = "Failed to deserialize the JSON body into the target type: missing field `name` at line 1 column 2";
  const { impl } = fakeFetch([{ response: () => textResponse(422, raw) }]);
  const client = makeClient(impl);

  await assert.rejects(client.request("POST", "/api/v1/accounts", { body: {} }), (error) => {
    assert.ok(error instanceof ApiError);
    assert.equal(error.status, 422);
    assert.equal(error.code, UNKNOWN_ERROR_CODE);
    assert.equal(error.message, raw);
    return true;
  });
});

test("empty error body maps to code UNKNOWN with an HTTP status message", async () => {
  const { impl } = fakeFetch([{ response: () => new Response(null, { status: 502 }) }]);
  const client = makeClient(impl);

  await assert.rejects(client.request("GET", "/api/v1/accounts"), (error) => {
    assert.ok(error instanceof ApiError);
    assert.equal(error.status, 502);
    assert.equal(error.code, UNKNOWN_ERROR_CODE);
    assert.equal(error.message, "HTTP 502");
    return true;
  });
});

test("JSON error body that is not the envelope maps to code UNKNOWN", async () => {
  const { impl } = fakeFetch([
    { response: () => jsonResponse(400, { error: "not the envelope" }) },
  ]);
  const client = makeClient(impl);

  await assert.rejects(client.request("GET", "/api/v1/accounts"), (error) => {
    assert.ok(error instanceof ApiError);
    assert.equal(error.code, UNKNOWN_ERROR_CODE);
    return true;
  });
});

test("parseErrorEnvelope accepts only envelope-shaped objects", () => {
  assert.equal(parseErrorEnvelope(null), null);
  assert.equal(parseErrorEnvelope("RATE_LIMITED"), null);
  assert.equal(parseErrorEnvelope(42), null);
  assert.equal(parseErrorEnvelope(["RATE_LIMITED"]), null);
  assert.equal(parseErrorEnvelope({ message: "no code" }), null);
  assert.equal(parseErrorEnvelope({ code: 429 }), null);

  assert.deepEqual(parseErrorEnvelope({ code: "NOT_FOUND", message: "account not found" }), {
    code: "NOT_FOUND",
    message: "account not found",
  });
  // Envelope without a details key stays without one (omitted, never null).
  assert.equal("details" in parseErrorEnvelope({ code: "X", message: "y" }), false);
  assert.deepEqual(
    parseErrorEnvelope({ code: "VALIDATION_ERROR", message: "m", details: { field: "name" } }),
    { code: "VALIDATION_ERROR", message: "m", details: { field: "name" } },
  );
});
