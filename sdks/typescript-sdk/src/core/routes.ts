/**
 * Route pinning and path expansion — the seam every typed service module sits on.
 *
 * `InfraPortalClient.request` takes `path: string`, so on their own the route
 * literals in a service module are unchecked text. A route renamed in a
 * service's `openapi.yaml` would leave the SDK calling a dead URL with `tsc`
 * perfectly green, and a route ADDED to a spec would simply never gain an SDK
 * method. The helpers here close both holes at compile time:
 *
 * - `satisfies readonly (keyof paths)[]` on a module's route tuple proves every
 *   route the SDK calls EXISTS in that service's generated spec types.
 * - `AssertNoUncoveredRoutes` proves the reverse direction: every route the spec
 *   declares, other than the `/health` and `/ready` probes, is covered by the
 *   module — so a spec that gains an endpoint fails the build naming it.
 * - `AssertNoParameters` pins the three list operations that deliberately take
 *   no query parameters, so a spec that gains a filter cannot leave the SDK
 *   silently unable to send it.
 * - `JsonBody` derives each method's response type from the operation's own
 *   declared success response instead of guessing a schema name. That matters
 *   here: `list` returns a `List<X>Response` envelope on accounts, contacts and
 *   audit, and a bare `X[]` array on activities, automation, integrations and
 *   opportunities. A module that assumed one shape for all seven would be
 *   wrong for four of them and `tsc` could not tell.
 *
 * All of these read the generated `paths` / `operations` interfaces, which
 * `npm run generate` produces from `<service>-service/openapi.yaml`. They guard
 * the SDK against the COMMITTED generated types; keeping those in step with the
 * specs is `npm run generate`'s job (run it, then check `git diff src/generated/`).
 */

/**
 * Infrastructure probes every service spec declares. They are not part of any
 * resource module's surface — callers reach them through
 * `client.request("GET", "/health")` — so route-coverage assertions exclude them.
 */
export type ProbePath = "/health" | "/ready";

/**
 * Compile-time assertion that a module covers every route in its spec.
 *
 * Instantiate it with the set of routes the spec declares minus the ones the
 * module handles. While that set is empty the alias is `never` and this is a
 * no-op; the moment a spec gains an uncovered route the argument stops
 * satisfying `extends never` and the build fails, naming the missing path.
 */
export type AssertNoUncoveredRoutes<Uncovered extends never> = Uncovered;

/**
 * Compile-time assertion helper. Instantiate it with a predicate alias that
 * should resolve to `true`; when the predicate resolves to `false` the
 * constraint fails and the error names the alias that made the claim.
 *
 * (The assertion cannot be hidden in a defaulted type parameter — TypeScript
 * checks a default against its constraint generically, so an unresolved
 * conditional widens to `boolean` and the helper fails to compile at its own
 * declaration rather than at the call site.)
 */
export type Assert<Check extends true> = Check;

/**
 * `true` when an operation declares no parameters at all.
 *
 * `listActivities`, `listWorkflows` and `listConnections` currently take no
 * query string, so their `list()` methods accept no argument. If one of those
 * specs later grows a filter, `"parameters"` becomes a key of the operation,
 * this predicate flips to `false`, and `Assert` fails the build rather than
 * leaving the SDK quietly unable to express the new parameter.
 */
export type DeclaresNoParameters<Op> = "parameters" extends keyof Op
  ? false
  : true;

/**
 * The `application/json` body an operation declares for `Status`.
 *
 * `Status` is constrained to the statuses the operation actually declares, so
 * asking for one it does not have is a build error naming the valid set rather
 * than a silent collapse to `never` — and `never` is assignable to everything,
 * which would let a method keep compiling while promising a value it can never
 * describe. A status that exists but carries no JSON body (a 204, or the
 * text/plain axum rejections) still resolves to `never`; methods for those
 * declare their response type directly instead of deriving it here.
 */
export type JsonBody<
  Op extends { responses: object },
  Status extends keyof Op["responses"],
> = Op["responses"][Status] extends {
  content: { "application/json": infer Body };
}
  ? Body
  : never;

/** The `{name}` placeholders in a path template, as a union of their names. */
export type PathParamNames<Template extends string> =
  Template extends `${string}{${infer Name}}${infer Rest}`
    ? Name | PathParamNames<Rest>
    : never;

/** The params object a template requires: one string per `{name}` placeholder. */
export type PathParams<Template extends string> = Record<
  PathParamNames<Template>,
  string
>;

const PLACEHOLDER = /\{([^{}]*)\}/g;

/**
 * Substitutes `{name}` placeholders in a spec path template with URL-encoded
 * values, e.g. `/api/v1/contacts/{id}` + `{ id }` -> `/api/v1/contacts/<id>`.
 *
 * Throws rather than emitting a malformed URL, because every failure mode here
 * produces a request that SUCCEEDS against the wrong endpoint: an empty id
 * collapses `/api/v1/contacts/{id}` to `/api/v1/contacts/`, which the gateway
 * routes to the collection — so `get("")` would return the full list and
 * `delete("")` would target the collection route rather than one record.
 */
export function expandPath<Template extends string>(
  template: Template,
  params: PathParams<Template>,
): string {
  const values = params as Record<string, unknown>;
  return template.replace(PLACEHOLDER, (_match: string, name: string) => {
    if (name === "") {
      throw new Error(`expandPath: empty placeholder in template "${template}"`);
    }
    if (!Object.prototype.hasOwnProperty.call(values, name)) {
      throw new Error(
        `expandPath: missing value for path parameter "${name}" in template "${template}"`,
      );
    }
    const value = values[name];
    if (typeof value !== "string") {
      throw new Error(
        `expandPath: path parameter "${name}" must be a string, received ${typeof value}`,
      );
    }
    if (value === "") {
      throw new Error(
        `expandPath: path parameter "${name}" must not be empty in template "${template}"`,
      );
    }
    return encodeURIComponent(value);
  });
}
