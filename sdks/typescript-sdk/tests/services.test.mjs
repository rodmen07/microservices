/**
 * Wiring coverage for every typed service module: each method must issue the
 * HTTP verb and the URL its spec route declares.
 *
 * The compile-time seam in src/core/routes.ts proves each module's route
 * literals EXIST in that service's generated spec types and that no spec route
 * is left uncovered. It cannot prove that `get()` uses the item route rather
 * than the collection route, or that `update()` sends PATCH rather than PUT —
 * a module could pin both routes correctly and still call the wrong one. These
 * tests close that half by recording the real request.
 *
 * The expected URLs are written out by hand rather than derived from the
 * module's own constants, so a wrong route literal cannot agree with itself.
 *
 * One case per method: node:test aborts a test at its first failed assertion,
 * so a table driven through a single test would report only the first break.
 */
import test from "node:test";
import assert from "node:assert/strict";
import {
  AccountsApi,
  ActivitiesApi,
  AuditApi,
  AutomationApi,
  ContactsApi,
  InfraPortalClient,
  IntegrationsApi,
  OpportunitiesApi,
} from "../dist/index.js";
import { fakeFetch, fakeSleep, jsonResponse } from "./helpers.mjs";

const BASE = "https://gateway.example.test";
const ID = "9c1d3e5f-7a2b-4c8d-b0e6-4f2a1c9d8e7b";

function noContent() {
  return new Response(null, { status: 204 });
}

/** Runs one API method against a stub fetch and returns the recorded call. */
async function callOnce(ApiClass, invoke, response = () => jsonResponse(200, {})) {
  const { impl, calls } = fakeFetch([{ response }]);
  const { sleep } = fakeSleep();
  const client = new InfraPortalClient({
    baseUrl: BASE,
    fetch: impl,
    sleep,
    token: "jwt-test",
  });
  await invoke(new ApiClass(client));
  assert.equal(calls.length, 1, "expected exactly one request");
  return calls[0];
}

/**
 * Declares the full expected surface of every module: one row per method, each
 * naming the verb and URL the service's openapi.yaml declares for it.
 */
const CASES = [
  // accounts-service
  ["accounts.list", AccountsApi, (a) => a.list(), "GET", "/api/v1/accounts"],
  [
    "accounts.list with query",
    AccountsApi,
    (a) => a.list({ limit: 25 }),
    "GET",
    "/api/v1/accounts?limit=25",
  ],
  ["accounts.get", AccountsApi, (a) => a.get(ID), "GET", `/api/v1/accounts/${ID}`],
  [
    "accounts.create",
    AccountsApi,
    (a) => a.create({ name: "Acme" }),
    "POST",
    "/api/v1/accounts",
    () => jsonResponse(201, {}),
  ],
  [
    "accounts.update",
    AccountsApi,
    (a) => a.update(ID, { name: "Acme 2" }),
    "PATCH",
    `/api/v1/accounts/${ID}`,
  ],
  [
    "accounts.delete",
    AccountsApi,
    (a) => a.delete(ID),
    "DELETE",
    `/api/v1/accounts/${ID}`,
    noContent,
  ],

  // contacts-service
  ["contacts.list", ContactsApi, (c) => c.list(), "GET", "/api/v1/contacts"],
  [
    "contacts.list with query",
    ContactsApi,
    (c) => c.list({ lifecycle_stage: "customer" }),
    "GET",
    "/api/v1/contacts?lifecycle_stage=customer",
  ],
  ["contacts.get", ContactsApi, (c) => c.get(ID), "GET", `/api/v1/contacts/${ID}`],
  [
    "contacts.create",
    ContactsApi,
    (c) => c.create({ first_name: "A", last_name: "B" }),
    "POST",
    "/api/v1/contacts",
    () => jsonResponse(201, {}),
  ],
  [
    "contacts.update",
    ContactsApi,
    (c) => c.update(ID, { first_name: "C" }),
    "PATCH",
    `/api/v1/contacts/${ID}`,
  ],
  [
    "contacts.delete",
    ContactsApi,
    (c) => c.delete(ID),
    "DELETE",
    `/api/v1/contacts/${ID}`,
    noContent,
  ],

  // opportunities-service
  [
    "opportunities.list",
    OpportunitiesApi,
    (o) => o.list(),
    "GET",
    "/api/v1/opportunities",
  ],
  [
    "opportunities.list with query",
    OpportunitiesApi,
    (o) => o.list({ owner_id: ID }),
    "GET",
    `/api/v1/opportunities?owner_id=${ID}`,
  ],
  [
    "opportunities.get",
    OpportunitiesApi,
    (o) => o.get(ID),
    "GET",
    `/api/v1/opportunities/${ID}`,
  ],
  [
    "opportunities.create",
    OpportunitiesApi,
    (o) => o.create({ name: "Deal", account_id: ID }),
    "POST",
    "/api/v1/opportunities",
    () => jsonResponse(201, {}),
  ],
  [
    "opportunities.update",
    OpportunitiesApi,
    (o) => o.update(ID, { stage: "won" }),
    "PATCH",
    `/api/v1/opportunities/${ID}`,
  ],
  [
    "opportunities.delete",
    OpportunitiesApi,
    (o) => o.delete(ID),
    "DELETE",
    `/api/v1/opportunities/${ID}`,
    noContent,
  ],

  // activities-service
  ["activities.list", ActivitiesApi, (a) => a.list(), "GET", "/api/v1/activities"],
  [
    "activities.get",
    ActivitiesApi,
    (a) => a.get(ID),
    "GET",
    `/api/v1/activities/${ID}`,
  ],
  [
    "activities.create",
    ActivitiesApi,
    (a) => a.create({ subject: "Call" }),
    "POST",
    "/api/v1/activities",
    () => jsonResponse(201, {}),
  ],
  [
    "activities.update",
    ActivitiesApi,
    (a) => a.update(ID, { subject: "Call back" }),
    "PATCH",
    `/api/v1/activities/${ID}`,
  ],
  [
    "activities.delete",
    ActivitiesApi,
    (a) => a.delete(ID),
    "DELETE",
    `/api/v1/activities/${ID}`,
    noContent,
  ],

  // automation-service (resource path is /workflows, not /automation)
  ["automation.list", AutomationApi, (a) => a.list(), "GET", "/api/v1/workflows"],
  [
    "automation.get",
    AutomationApi,
    (a) => a.get(ID),
    "GET",
    `/api/v1/workflows/${ID}`,
  ],
  [
    "automation.create",
    AutomationApi,
    (a) => a.create({ name: "Nightly" }),
    "POST",
    "/api/v1/workflows",
    () => jsonResponse(201, {}),
  ],
  [
    "automation.update",
    AutomationApi,
    (a) => a.update(ID, { name: "Nightly v2" }),
    "PATCH",
    `/api/v1/workflows/${ID}`,
  ],
  [
    "automation.delete",
    AutomationApi,
    (a) => a.delete(ID),
    "DELETE",
    `/api/v1/workflows/${ID}`,
    noContent,
  ],

  // integrations-service (two-segment resource path)
  [
    "integrations.list",
    IntegrationsApi,
    (i) => i.list(),
    "GET",
    "/api/v1/integrations/connections",
  ],
  [
    "integrations.get",
    IntegrationsApi,
    (i) => i.get(ID),
    "GET",
    `/api/v1/integrations/connections/${ID}`,
  ],
  [
    "integrations.create",
    IntegrationsApi,
    (i) => i.create({ provider: "github" }),
    "POST",
    "/api/v1/integrations/connections",
    () => jsonResponse(201, {}),
  ],
  [
    "integrations.update",
    IntegrationsApi,
    (i) => i.update(ID, { provider: "gitlab" }),
    "PATCH",
    `/api/v1/integrations/connections/${ID}`,
  ],
  [
    "integrations.delete",
    IntegrationsApi,
    (i) => i.delete(ID),
    "DELETE",
    `/api/v1/integrations/connections/${ID}`,
    noContent,
  ],

  // audit-service (append-only: list + ingest, no get/update/delete)
  ["audit.list", AuditApi, (a) => a.list(), "GET", "/api/v1/audit-events"],
  [
    "audit.list with query",
    AuditApi,
    (a) => a.list({ entity_type: "account" }),
    "GET",
    "/api/v1/audit-events?entity_type=account",
  ],
  [
    "audit.ingest",
    AuditApi,
    (a) => a.ingest({ entity_type: "account", entity_id: ID, action: "created" }),
    "POST",
    "/api/v1/audit-events",
    () => jsonResponse(201, {}),
  ],
];

for (const [name, ApiClass, invoke, expectedMethod, expectedPath, response] of CASES) {
  test(`${name} issues ${expectedMethod} ${expectedPath}`, async () => {
    const call = await callOnce(ApiClass, invoke, response);
    assert.equal(call.init.method, expectedMethod);
    assert.equal(call.url, `${BASE}${expectedPath}`);
  });
}

test("every typed service module is exercised by the table above", () => {
  const covered = new Set(CASES.map(([name]) => name.split(".")[0]));
  assert.deepEqual(
    [...covered].sort(),
    [
      "accounts",
      "activities",
      "audit",
      "automation",
      "contacts",
      "integrations",
      "opportunities",
    ],
    "a module was added without wiring cases; add its rows to CASES",
  );
});

test("an empty id is rejected before it can target the collection route", async () => {
  const { impl, calls } = fakeFetch([]);
  const { sleep } = fakeSleep();
  const client = new InfraPortalClient({ baseUrl: BASE, fetch: impl, sleep });
  const contacts = new ContactsApi(client);

  assert.throws(() => contacts.get(""), /must not be empty/);
  assert.equal(calls.length, 0, "no request may be issued for an empty id");
});
