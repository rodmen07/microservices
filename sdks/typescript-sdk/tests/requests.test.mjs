import test from "node:test";
import assert from "node:assert/strict";
import { AccountsApi, InfraPortalClient } from "../dist/index.js";
import { fakeFetch, fakeSleep, jsonResponse } from "./helpers.mjs";

const BASE = "https://gateway.example.test";

function makeClient(fetchImpl, options = {}) {
  const { sleep } = fakeSleep();
  return new InfraPortalClient({ baseUrl: BASE, fetch: fetchImpl, sleep, ...options });
}

test("injects the bearer token and Accept header", async () => {
  const { impl, calls } = fakeFetch([{ response: () => jsonResponse(200, {}) }]);
  const client = makeClient(impl, { token: "jwt-abc" });

  await client.request("GET", "/api/v1/accounts");
  assert.equal(calls[0].init.headers.Authorization, "Bearer jwt-abc");
  assert.equal(calls[0].init.headers.Accept, "application/json");
});

test("omits Authorization when no token is configured", async () => {
  const { impl, calls } = fakeFetch([{ response: () => jsonResponse(200, {}) }]);
  const client = makeClient(impl);

  await client.request("GET", "/health");
  assert.equal("Authorization" in calls[0].init.headers, false);
});

test("setToken replaces the bearer token on later requests", async () => {
  const { impl, calls } = fakeFetch([
    { response: () => jsonResponse(200, {}) },
    { response: () => jsonResponse(200, {}) },
  ]);
  const client = makeClient(impl, { token: "old" });

  await client.request("GET", "/api/v1/accounts");
  client.setToken("new");
  await client.request("GET", "/api/v1/accounts");
  assert.equal(calls[0].init.headers.Authorization, "Bearer old");
  assert.equal(calls[1].init.headers.Authorization, "Bearer new");
});

test("serializes JSON bodies and sets Content-Type", async () => {
  const { impl, calls } = fakeFetch([{ response: () => jsonResponse(201, { id: "a1" }) }]);
  const client = makeClient(impl, { token: "t" });

  await client.request("POST", "/api/v1/accounts", { body: { name: "Globex", domain: null } });
  assert.equal(calls[0].init.method, "POST");
  assert.equal(calls[0].init.headers["Content-Type"], "application/json");
  assert.equal(calls[0].init.body, '{"name":"Globex","domain":null}');
});

test("builds query strings and skips null/undefined values", async () => {
  const { impl, calls } = fakeFetch([
    { response: () => jsonResponse(200, { data: [], total: 0, limit: 10, offset: 0 }) },
  ]);
  const client = makeClient(impl, { token: "t" });

  await client.request("GET", "/api/v1/accounts", {
    query: { limit: 10, status: "active", q: undefined, owner_id: null },
  });
  assert.equal(calls[0].url, `${BASE}/api/v1/accounts?limit=10&status=active`);
});

test("trims trailing slashes from baseUrl", async () => {
  const { impl, calls } = fakeFetch([{ response: () => jsonResponse(200, {}) }]);
  const { sleep } = fakeSleep();
  const client = new InfraPortalClient({ baseUrl: `${BASE}//`, fetch: impl, sleep });

  await client.request("GET", "/health");
  assert.equal(calls[0].url, `${BASE}/health`);
});

test("204 responses resolve with undefined data", async () => {
  const { impl } = fakeFetch([{ response: () => new Response(null, { status: 204 }) }]);
  const client = makeClient(impl, { token: "t" });

  const res = await client.request("DELETE", "/api/v1/accounts/a1");
  assert.equal(res.status, 204);
  assert.equal(res.data, undefined);
});

test("AccountsApi maps methods to the documented routes", async () => {
  const account = {
    id: "a1",
    owner_id: "u1",
    name: "Globex Corporation",
    domain: null,
    status: "active",
    created_at: "2026-07-18T14:30:00Z",
    updated_at: "2026-07-18T14:30:00Z",
  };
  const { impl, calls } = fakeFetch([
    { response: () => jsonResponse(200, { data: [account], total: 1, limit: 50, offset: 0 }) },
    { response: () => jsonResponse(200, account) },
    { response: () => jsonResponse(201, account) },
    { response: () => jsonResponse(200, { ...account, status: "inactive" }) },
    { response: () => new Response(null, { status: 204 }) },
  ]);
  const client = makeClient(impl, { token: "t" });
  const accounts = new AccountsApi(client);

  const listed = await accounts.list({ status: "active" });
  assert.equal(listed.data.total, 1);
  await accounts.get("a1");
  await accounts.create({ name: "Globex Corporation" });
  await accounts.update("a1", { status: "inactive" });
  const deleted = await accounts.delete("a/1");

  assert.equal(deleted.status, 204);
  assert.deepEqual(
    calls.map((c) => [c.init.method, c.url]),
    [
      ["GET", `${BASE}/api/v1/accounts?status=active`],
      ["GET", `${BASE}/api/v1/accounts/a1`],
      ["POST", `${BASE}/api/v1/accounts`],
      ["PATCH", `${BASE}/api/v1/accounts/a1`],
      ["DELETE", `${BASE}/api/v1/accounts/a%2F1`],
    ],
  );
});
