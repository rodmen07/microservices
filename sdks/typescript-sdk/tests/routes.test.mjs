/**
 * expandPath: the runtime half of the route seam.
 *
 * Every rejection here exists because the malformed URL it prevents would
 * SUCCEED against the wrong endpoint rather than fail loudly — an empty or
 * missing id collapses `/api/v1/contacts/{id}` to `/api/v1/contacts/`, which
 * the gateway routes to the collection.
 *
 * One clause per test case: node:test aborts a test at its first failed
 * assertion, so bundling these would let the earliest failure stand in for
 * every later one.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { expandPath } from "../dist/index.js";

test("substitutes a single placeholder", () => {
  assert.equal(
    expandPath("/api/v1/contacts/{id}", { id: "abc" }),
    "/api/v1/contacts/abc",
  );
});

test("substitutes every placeholder in a multi-parameter template", () => {
  assert.equal(
    expandPath("/api/v1/projects/{project_id}/milestones/{id}", {
      project_id: "p1",
      id: "m2",
    }),
    "/api/v1/projects/p1/milestones/m2",
  );
});

test("leaves a template with no placeholders untouched", () => {
  assert.equal(expandPath("/api/v1/contacts", {}), "/api/v1/contacts");
});

test("URL-encodes a value so it cannot inject extra path segments", () => {
  assert.equal(
    expandPath("/api/v1/contacts/{id}", { id: "a/../b" }),
    "/api/v1/contacts/a%2F..%2Fb",
  );
});

test("URL-encodes a value so it cannot inject a query string", () => {
  assert.equal(
    expandPath("/api/v1/contacts/{id}", { id: "x?admin=1" }),
    "/api/v1/contacts/x%3Fadmin%3D1",
  );
});

test("does not touch text outside the braces", () => {
  assert.equal(
    expandPath("/api/v1/integrations/connections/{id}", { id: "c1" }),
    "/api/v1/integrations/connections/c1",
  );
});

test("throws when a required parameter is missing", () => {
  assert.throws(
    () => expandPath("/api/v1/contacts/{id}", {}),
    /missing value for path parameter "id"/,
  );
});

test("throws when a parameter is the empty string, rather than targeting the collection", () => {
  assert.throws(
    () => expandPath("/api/v1/contacts/{id}", { id: "" }),
    /path parameter "id" must not be empty/,
  );
});

test("throws when a parameter is not a string", () => {
  assert.throws(
    () => expandPath("/api/v1/contacts/{id}", { id: 42 }),
    /path parameter "id" must be a string, received number/,
  );
});

test("throws on an empty placeholder rather than emitting it literally", () => {
  assert.throws(
    () => expandPath("/api/v1/contacts/{}", {}),
    /empty placeholder in template/,
  );
});

test("names the offending template in its error, so the failing route is identifiable", () => {
  assert.throws(
    () => expandPath("/api/v1/workflows/{id}", {}),
    /template "\/api\/v1\/workflows\/\{id\}"/,
  );
});
