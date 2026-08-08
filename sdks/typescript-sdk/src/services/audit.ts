/**
 * Typed audit-service module: list and ingest over /api/v1/audit-events, using
 * the types generated from audit-service/openapi.yaml.
 *
 * The audit log is append-only, so this module deliberately has no get/update/
 * delete: the spec declares exactly one path with exactly two operations. That
 * is not an omission to fill in later — `_AllRoutesCovered` proves the module
 * covers every route the spec declares, so if audit-service ever gains a
 * per-event route the build fails naming it.
 *
 * Authorization differs by operation (docs/API.md, PR #92): listing is
 * admin-only, while ingest accepts an admin OR a service token — which is why
 * the generated spec carries two distinct 403 responses for this service.
 */
import type {
  ApiResponse,
  InfraPortalClient,
  RequestOptions,
} from "../core/client.js";
import type {
  AssertNoUncoveredRoutes,
  JsonBody,
  ProbePath,
} from "../core/routes.js";
import type { components, operations, paths } from "../generated/audit.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = ["/api/v1/audit-events"] as const satisfies readonly (keyof paths)[];

const [AUDIT_EVENTS] = ROUTES;

/** Fails the build if audit-service/openapi.yaml gains a route this module does
 * not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

export type AuditEvent = components["schemas"]["AuditEvent"];
export type CreateAuditEventRequest =
  components["schemas"]["CreateAuditEventRequest"];
export type ListAuditEventsResponse = JsonBody<
  operations["listAuditEvents"],
  200
>;
export type ListAuditEventsQuery = NonNullable<
  operations["listAuditEvents"]["parameters"]["query"]
>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class AuditApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/audit-events. Admin-only. Paginated, and filterable by entity,
   * actor, action, and a created_after/created_before window. */
  list(
    query?: ListAuditEventsQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<ListAuditEventsResponse>> {
    return this.client.request<ListAuditEventsResponse>("GET", AUDIT_EVENTS, {
      ...options,
      query,
    });
  }

  /** POST /api/v1/audit-events. Records one event; returns 201. Accepts an
   * admin or a service token. Retried automatically only on 429, because a
   * retried network error would double-record the event; pass
   * `idempotent: true` only if your consumer tolerates duplicates. */
  ingest(
    body: CreateAuditEventRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<AuditEvent>> {
    return this.client.request<AuditEvent>("POST", AUDIT_EVENTS, {
      ...options,
      body,
    });
  }
}
