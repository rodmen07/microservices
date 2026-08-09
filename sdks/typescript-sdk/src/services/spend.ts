/**
 * Typed spend-service module: cost records, their summary, and the four
 * provider sync triggers, using the types generated from
 * spend-service/openapi.yaml.
 *
 * Three shapes here are unlike the CRM services:
 *
 * - `list` returns the paginated `ListSpendResponse` envelope, where the other
 *   three services in this slice (projects, reporting, search) all return bare
 *   arrays. The response types are derived per operation so that difference is
 *   carried by the types instead of assumed away.
 * - The four `/sync/*` routes are POSTs with NO request body and no parameters.
 *   They are the only write on the platform whose payload comes from an external
 *   provider rather than the caller, so the methods take no arguments and the
 *   client sends no `Content-Type`.
 * - A 403 from `update` or `delete` is a RECORD-SOURCE guard, not a role check:
 *   only records with `source: "manual"` may be edited, so an admin token does
 *   not make a synced record writable. Do not read it as the CRM services' 403.
 */
import type {
  ApiResponse,
  InfraPortalClient,
  RequestOptions,
} from "../core/client.js";
import { expandPath } from "../core/routes.js";
import type {
  Assert,
  AssertNoUncoveredRoutes,
  DeclaresNoParameters,
  JsonBody,
  ProbePath,
} from "../core/routes.js";
import type { components, operations, paths } from "../generated/spend.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/spend",
  "/api/v1/spend/summary",
  "/api/v1/spend/sync/gcp",
  "/api/v1/spend/sync/flyio",
  "/api/v1/spend/sync/github",
  "/api/v1/spend/sync/aws",
  "/api/v1/spend/{id}",
] as const satisfies readonly (keyof paths)[];

const [SPEND, SUMMARY, SYNC_GCP, SYNC_FLYIO, SYNC_GITHUB, SYNC_AWS, RECORD] =
  ROUTES;

/** Fails the build if spend-service/openapi.yaml gains a route this module
 * does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

/** Each sync trigger takes nothing today. A spec that grows a parameter — a
 * date window, a project selector — fails the build here rather than leaving
 * the method silently unable to send it. One assertion per provider, so one
 * spec change reports as one error naming the provider it belongs to. */
type _SyncGcpDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["syncGcp"]>
>;
type _SyncFlyioDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["syncFlyio"]>
>;
type _SyncGithubDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["syncGithub"]>
>;
type _SyncAwsDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["syncAws"]>
>;

export type SpendRecord = components["schemas"]["SpendRecord"];
export type SpendSummary = components["schemas"]["SpendSummary"];
export type SyncResult = components["schemas"]["SyncResult"];
export type CreateSpendRequest = components["schemas"]["CreateSpendRequest"];
export type UpdateSpendRequest = components["schemas"]["UpdateSpendRequest"];
export type ListSpendQuery = NonNullable<
  operations["listSpend"]["parameters"]["query"]
>;
export type ListSpendResponse = JsonBody<operations["listSpend"], 200>;
export type SpendSummaryQuery = NonNullable<
  operations["getSpendSummary"]["parameters"]["query"]
>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class SpendApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/spend. Paginated envelope (`limit`/`offset`), optionally
   * narrowed by `platform`, `source`, and a `date_from`/`date_to` window. */
  list(
    query?: ListSpendQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<ListSpendResponse>> {
    return this.client.request<ListSpendResponse>("GET", SPEND, {
      ...options,
      query,
    });
  }

  /** POST /api/v1/spend. Returns 201 and a record with `source: "manual"`,
   * which is what later makes it editable. Retried automatically only on 429. */
  create(
    body: CreateSpendRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<SpendRecord>> {
    return this.client.request<SpendRecord>("POST", SPEND, {
      ...options,
      body,
    });
  }

  /** GET /api/v1/spend/summary. Totals over the optional date window. */
  summary(
    query?: SpendSummaryQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<SpendSummary>> {
    return this.client.request<SpendSummary>("GET", SUMMARY, {
      ...options,
      query,
    });
  }

  /** POST /api/v1/spend/sync/gcp. Pulls the GCP billing export; no body. */
  syncGcp(options?: CallOptions): Promise<ApiResponse<SyncResult>> {
    return this.client.request<SyncResult>("POST", SYNC_GCP, { ...options });
  }

  /** POST /api/v1/spend/sync/flyio. Pulls Fly.io costs; no body. */
  syncFlyio(options?: CallOptions): Promise<ApiResponse<SyncResult>> {
    return this.client.request<SyncResult>("POST", SYNC_FLYIO, { ...options });
  }

  /** POST /api/v1/spend/sync/github. Pulls GitHub billing; no body. */
  syncGithub(options?: CallOptions): Promise<ApiResponse<SyncResult>> {
    return this.client.request<SyncResult>("POST", SYNC_GITHUB, { ...options });
  }

  /** POST /api/v1/spend/sync/aws. Pulls AWS Cost Explorer; no body. */
  syncAws(options?: CallOptions): Promise<ApiResponse<SyncResult>> {
    return this.client.request<SyncResult>("POST", SYNC_AWS, { ...options });
  }

  /** GET /api/v1/spend/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<SpendRecord>> {
    return this.client.request<SpendRecord>("GET", expandPath(RECORD, { id }), {
      ...options,
    });
  }

  /** PATCH /api/v1/spend/{id}. Manual records only: a synced record answers 403
   * regardless of role. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateSpendRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<SpendRecord>> {
    return this.client.request<SpendRecord>(
      "PATCH",
      expandPath(RECORD, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/spend/{id}. Manual records only, same 403 guard as update.
   * Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(RECORD, { id }), {
      ...options,
    });
  }
}
