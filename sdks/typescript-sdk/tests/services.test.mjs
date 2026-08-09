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
  ProjectsApi,
  ReportingApi,
  SearchApi,
  SpendApi,
} from "../dist/index.js";
import { fakeFetch, fakeSleep, jsonResponse } from "./helpers.mjs";

const BASE = "https://gateway.example.test";
const ID = "9c1d3e5f-7a2b-4c8d-b0e6-4f2a1c9d8e7b";

/**
 * projects-service nests resources, and a sub-resource is created under its
 * parent but addressed afterwards at its own top-level route. Every level gets
 * a DISTINCT id here so a method that interpolates the wrong one — a milestone
 * id into the project route, a project id into the milestone route — produces a
 * URL that differs from the expectation instead of one that happens to match.
 */
const PROJECT_ID = "1a2b3c4d-0000-4000-8000-000000000001";
const MILESTONE_ID = "1a2b3c4d-0000-4000-8000-000000000002";
const DELIVERABLE_ID = "1a2b3c4d-0000-4000-8000-000000000003";
const LINK_ID = "1a2b3c4d-0000-4000-8000-000000000004";
const ENTITY_ID = "1a2b3c4d-0000-4000-8000-000000000005";

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

  // projects-service: projects plus four nested sub-resources. Note which id
  // each row passes — creates take the PARENT id, updates and deletes take the
  // sub-resource's OWN id, and the two routes are not interchangeable.
  ["projects.list", ProjectsApi, (p) => p.list(), "GET", "/api/v1/projects"],
  [
    "projects.create",
    ProjectsApi,
    (p) => p.create({ name: "Portal", account_id: ID }),
    "POST",
    "/api/v1/projects",
    () => jsonResponse(201, {}),
  ],
  [
    "projects.get",
    ProjectsApi,
    (p) => p.get(PROJECT_ID),
    "GET",
    `/api/v1/projects/${PROJECT_ID}`,
  ],
  [
    "projects.update",
    ProjectsApi,
    (p) => p.update(PROJECT_ID, { status: "paused" }),
    "PATCH",
    `/api/v1/projects/${PROJECT_ID}`,
  ],
  [
    "projects.delete",
    ProjectsApi,
    (p) => p.delete(PROJECT_ID),
    "DELETE",
    `/api/v1/projects/${PROJECT_ID}`,
    noContent,
  ],
  [
    "projects.listMilestones",
    ProjectsApi,
    (p) => p.listMilestones(PROJECT_ID),
    "GET",
    `/api/v1/projects/${PROJECT_ID}/milestones`,
  ],
  [
    "projects.createMilestone",
    ProjectsApi,
    (p) => p.createMilestone(PROJECT_ID, { title: "Kickoff" }),
    "POST",
    `/api/v1/projects/${PROJECT_ID}/milestones`,
    () => jsonResponse(201, {}),
  ],
  [
    "projects.updateMilestone",
    ProjectsApi,
    (p) => p.updateMilestone(MILESTONE_ID, { title: "Kickoff v2" }),
    "PATCH",
    `/api/v1/milestones/${MILESTONE_ID}`,
  ],
  [
    "projects.deleteMilestone",
    ProjectsApi,
    (p) => p.deleteMilestone(MILESTONE_ID),
    "DELETE",
    `/api/v1/milestones/${MILESTONE_ID}`,
    noContent,
  ],
  [
    "projects.listDeliverables",
    ProjectsApi,
    (p) => p.listDeliverables(MILESTONE_ID),
    "GET",
    `/api/v1/milestones/${MILESTONE_ID}/deliverables`,
  ],
  [
    "projects.createDeliverable",
    ProjectsApi,
    (p) => p.createDeliverable(MILESTONE_ID, { title: "Spec" }),
    "POST",
    `/api/v1/milestones/${MILESTONE_ID}/deliverables`,
    () => jsonResponse(201, {}),
  ],
  [
    "projects.updateDeliverable",
    ProjectsApi,
    (p) => p.updateDeliverable(DELIVERABLE_ID, { title: "Spec v2" }),
    "PATCH",
    `/api/v1/deliverables/${DELIVERABLE_ID}`,
  ],
  [
    "projects.deleteDeliverable",
    ProjectsApi,
    (p) => p.deleteDeliverable(DELIVERABLE_ID),
    "DELETE",
    `/api/v1/deliverables/${DELIVERABLE_ID}`,
    noContent,
  ],
  [
    "projects.listMessages",
    ProjectsApi,
    (p) => p.listMessages(PROJECT_ID),
    "GET",
    `/api/v1/projects/${PROJECT_ID}/messages`,
  ],
  [
    "projects.createMessage",
    ProjectsApi,
    (p) => p.createMessage(PROJECT_ID, { body: "Hello" }),
    "POST",
    `/api/v1/projects/${PROJECT_ID}/messages`,
    () => jsonResponse(201, {}),
  ],
  [
    "projects.listLinks",
    ProjectsApi,
    (p) => p.listLinks(PROJECT_ID),
    "GET",
    `/api/v1/projects/${PROJECT_ID}/links`,
  ],
  [
    "projects.createLink",
    ProjectsApi,
    (p) => p.createLink(PROJECT_ID, { label: "Repo", url: "https://x.test" }),
    "POST",
    `/api/v1/projects/${PROJECT_ID}/links`,
    () => jsonResponse(201, {}),
  ],
  [
    "projects.deleteLink",
    ProjectsApi,
    (p) => p.deleteLink(LINK_ID),
    "DELETE",
    `/api/v1/links/${LINK_ID}`,
    noContent,
  ],
  [
    "projects.listEmails",
    ProjectsApi,
    (p) => p.listEmails(PROJECT_ID),
    "GET",
    `/api/v1/projects/${PROJECT_ID}/emails`,
  ],
  [
    "projects.syncEmails",
    ProjectsApi,
    (p) => p.syncEmails(PROJECT_ID, { emails: [] }),
    "POST",
    `/api/v1/projects/${PROJECT_ID}/emails/sync`,
  ],

  // reporting-service (two dashboards on different routes, plus reports CRUD)
  ["reporting.dashboard", ReportingApi, (r) => r.dashboard(), "GET", "/api/v1/dashboard"],
  [
    "reporting.dashboard with query",
    ReportingApi,
    (r) => r.dashboard({ user_id: ID }),
    "GET",
    `/api/v1/dashboard?user_id=${ID}`,
  ],
  [
    "reporting.dashboardSummary",
    ReportingApi,
    (r) => r.dashboardSummary(),
    "GET",
    "/api/v1/reports/dashboard",
  ],
  ["reporting.list", ReportingApi, (r) => r.list(), "GET", "/api/v1/reports"],
  [
    "reporting.create",
    ReportingApi,
    (r) => r.create({ name: "Pipeline", metric: "opportunities.pipeline_value" }),
    "POST",
    "/api/v1/reports",
    () => jsonResponse(201, {}),
  ],
  [
    "reporting.export",
    ReportingApi,
    (r) => r.export(),
    "GET",
    "/api/v1/reports/export",
  ],
  [
    "reporting.export with format",
    ReportingApi,
    (r) => r.export({ format: "csv" }),
    "GET",
    "/api/v1/reports/export?format=csv",
  ],
  ["reporting.get", ReportingApi, (r) => r.get(ID), "GET", `/api/v1/reports/${ID}`],
  [
    "reporting.update",
    ReportingApi,
    (r) => r.update(ID, { name: "Pipeline v2" }),
    "PATCH",
    `/api/v1/reports/${ID}`,
  ],
  [
    "reporting.delete",
    ReportingApi,
    (r) => r.delete(ID),
    "DELETE",
    `/api/v1/reports/${ID}`,
    noContent,
  ],

  // search-service (query endpoint plus the document index behind it)
  [
    "search.search",
    SearchApi,
    (s) => s.search({ q: "acme" }),
    "GET",
    "/api/v1/search?q=acme",
  ],
  [
    "search.listDocuments",
    SearchApi,
    (s) => s.listDocuments(),
    "GET",
    "/api/v1/search/documents",
  ],
  [
    "search.indexDocument",
    SearchApi,
    (s) => s.indexDocument({ entity_type: "account", entity_id: ID, title: "Acme" }),
    "POST",
    "/api/v1/search/documents",
    () => jsonResponse(201, {}),
  ],
  [
    "search.get",
    SearchApi,
    (s) => s.get(ID),
    "GET",
    `/api/v1/search/documents/${ID}`,
  ],
  [
    "search.update",
    SearchApi,
    (s) => s.update(ID, { entity_type: "account", entity_id: ID, title: "Acme 2" }),
    "PATCH",
    `/api/v1/search/documents/${ID}`,
  ],
  [
    "search.delete",
    SearchApi,
    (s) => s.delete(ID),
    "DELETE",
    `/api/v1/search/documents/${ID}`,
    noContent,
  ],
  [
    "search.deleteByEntity",
    SearchApi,
    (s) => s.deleteByEntity(ENTITY_ID),
    "DELETE",
    `/api/v1/search/documents/by-entity/${ENTITY_ID}`,
    noContent,
  ],

  // spend-service (paginated envelope on list, four bodyless sync triggers)
  ["spend.list", SpendApi, (s) => s.list(), "GET", "/api/v1/spend"],
  [
    "spend.list with query",
    SpendApi,
    (s) => s.list({ limit: 25, platform: "gcp" }),
    "GET",
    "/api/v1/spend?limit=25&platform=gcp",
  ],
  [
    "spend.create",
    SpendApi,
    (s) => s.create({ platform: "gcp", amount: 12.5 }),
    "POST",
    "/api/v1/spend",
    () => jsonResponse(201, {}),
  ],
  [
    "spend.summary",
    SpendApi,
    (s) => s.summary({ date_from: "2026-08-01" }),
    "GET",
    "/api/v1/spend/summary?date_from=2026-08-01",
  ],
  ["spend.syncGcp", SpendApi, (s) => s.syncGcp(), "POST", "/api/v1/spend/sync/gcp"],
  [
    "spend.syncFlyio",
    SpendApi,
    (s) => s.syncFlyio(),
    "POST",
    "/api/v1/spend/sync/flyio",
  ],
  [
    "spend.syncGithub",
    SpendApi,
    (s) => s.syncGithub(),
    "POST",
    "/api/v1/spend/sync/github",
  ],
  ["spend.syncAws", SpendApi, (s) => s.syncAws(), "POST", "/api/v1/spend/sync/aws"],
  ["spend.get", SpendApi, (s) => s.get(ID), "GET", `/api/v1/spend/${ID}`],
  [
    "spend.update",
    SpendApi,
    (s) => s.update(ID, { amount: 13 }),
    "PATCH",
    `/api/v1/spend/${ID}`,
  ],
  [
    "spend.delete",
    SpendApi,
    (s) => s.delete(ID),
    "DELETE",
    `/api/v1/spend/${ID}`,
    noContent,
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
      "projects",
      "reporting",
      "search",
      "spend",
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

test("an empty parent id is rejected before it can target a sibling route", async () => {
  // `/api/v1/projects/{project_id}/milestones` with an empty project_id
  // collapses to `/api/v1/projects//milestones`, which is a different route
  // rather than a 404 — the nested-route form of the same hazard.
  const { impl, calls } = fakeFetch([]);
  const { sleep } = fakeSleep();
  const client = new InfraPortalClient({ baseUrl: BASE, fetch: impl, sleep });
  const projects = new ProjectsApi(client);

  assert.throws(() => projects.listMilestones(""), /must not be empty/);
  assert.equal(calls.length, 0, "no request may be issued for an empty id");
});

test("a spend sync POST sends no request body", async () => {
  // The four /sync/* routes take their payload from the provider, not the
  // caller. Sending `{}` would be a body the spec does not declare.
  const call = await callOnce(SpendApi, (s) => s.syncGcp());
  assert.equal(call.init.body, undefined);
});

test("a spend sync POST sends no Content-Type header", async () => {
  const call = await callOnce(SpendApi, (s) => s.syncGcp());
  assert.equal(call.init.headers["Content-Type"], undefined);
});

test("reporting.export hands back CSV text unparsed when format is csv", async () => {
  // The server switches representation on the `format` query parameter, so the
  // JSON-shaped return type alone would be a lie for this call. The union type
  // `ExportReportsResponse` exists because of this runtime behaviour.
  const csv = "id,name,metric\r\n1,Pipeline,opportunities.pipeline_value\r\n";
  const { impl } = fakeFetch([
    {
      response: () =>
        new Response(csv, {
          status: 200,
          headers: {
            "Content-Type": "text/csv",
            "Content-Disposition": "attachment; filename=reports-export-20260808.csv",
          },
        }),
    },
  ]);
  const { sleep } = fakeSleep();
  const client = new InfraPortalClient({ baseUrl: BASE, fetch: impl, sleep });

  const response = await new ReportingApi(client).export({ format: "csv" });
  assert.equal(response.data, csv);
});

test("reporting.export parses the JSON representation when format is json", async () => {
  const reports = [{ id: ID, name: "Pipeline" }];
  const { impl } = fakeFetch([{ response: () => jsonResponse(200, reports) }]);
  const { sleep } = fakeSleep();
  const client = new InfraPortalClient({ baseUrl: BASE, fetch: impl, sleep });

  const response = await new ReportingApi(client).export({ format: "json" });
  assert.deepEqual(response.data, reports);
});
