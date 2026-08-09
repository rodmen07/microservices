/**
 * Typed reporting-service module: the two dashboard views plus CRUD and export
 * over /api/v1/reports, using the types generated from
 * reporting-service/openapi.yaml.
 *
 * Two shapes here are unlike the CRM services:
 *
 * - There are TWO dashboards on different routes. `/api/v1/dashboard` is the
 *   cross-service aggregate (it fans out to accounts, contacts, opportunities
 *   and activities, and returns null for any sibling whose *_SERVICE_URL is
 *   unset), while `/api/v1/reports/dashboard` is the local summary. They return
 *   different schemas, so they are separate methods rather than one with a flag.
 * - `export` answers with CSV or JSON depending on its `format` query parameter,
 *   so its return type is derived with `AnyBody` rather than `JsonBody`. See the
 *   note on `exportReports` below.
 */
import type {
  ApiResponse,
  InfraPortalClient,
  RequestOptions,
} from "../core/client.js";
import { expandPath } from "../core/routes.js";
import type {
  AnyBody,
  Assert,
  AssertNoUncoveredRoutes,
  DeclaresNoParameters,
  JsonBody,
  ProbePath,
} from "../core/routes.js";
import type {
  components,
  operations,
  paths,
} from "../generated/reporting.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/dashboard",
  "/api/v1/reports/dashboard",
  "/api/v1/reports",
  "/api/v1/reports/export",
  "/api/v1/reports/{id}",
] as const satisfies readonly (keyof paths)[];

const [DASHBOARD, REPORTS_DASHBOARD, REPORTS, REPORTS_EXPORT, REPORT] = ROUTES;

/** Fails the build if reporting-service/openapi.yaml gains a route this
 * module does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

/** Both no-argument reads: a spec that grows a filter for either fails here
 * rather than leaving the SDK unable to express it. */
type _SummaryDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["getDashboardSummary"]>
>;
type _ListDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["listReports"]>
>;

export type DashboardView = components["schemas"]["DashboardView"];
export type DashboardSummary = components["schemas"]["DashboardSummary"];
export type SavedReport = components["schemas"]["SavedReport"];
export type CreateReportRequest = components["schemas"]["CreateReportRequest"];
export type UpdateReportRequest = components["schemas"]["UpdateReportRequest"];
export type GetDashboardQuery = NonNullable<
  operations["getDashboard"]["parameters"]["query"]
>;
export type ListReportsResponse = JsonBody<operations["listReports"], 200>;
export type ExportReportsQuery = NonNullable<
  operations["exportReports"]["parameters"]["query"]
>;
/** JSON array or CSV text, whichever `format` selected — see `export` below. */
export type ExportReportsResponse = AnyBody<operations["exportReports"], 200>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class ReportingApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/dashboard. Cross-service aggregate; each sibling total is null
   * when that service's URL is unset or its call failed. `user_id` scopes the
   * reports count and the owner filter sent downstream, never `core_metrics`. */
  dashboard(
    query?: GetDashboardQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<DashboardView>> {
    return this.client.request<DashboardView>("GET", DASHBOARD, {
      ...options,
      query,
    });
  }

  /** GET /api/v1/reports/dashboard. Local summary only: saved-report count plus
   * the sorted distinct metric names. Admin-only, so it always covers all
   * reports. */
  dashboardSummary(
    options?: CallOptions,
  ): Promise<ApiResponse<DashboardSummary>> {
    return this.client.request<DashboardSummary>("GET", REPORTS_DASHBOARD, {
      ...options,
    });
  }

  /** GET /api/v1/reports. Returns a bare array, not a paginated envelope. */
  list(options?: CallOptions): Promise<ApiResponse<ListReportsResponse>> {
    return this.client.request<ListReportsResponse>("GET", REPORTS, {
      ...options,
    });
  }

  /** POST /api/v1/reports. Returns 201. Retried automatically only on 429. */
  create(
    body: CreateReportRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<SavedReport>> {
    return this.client.request<SavedReport>("POST", REPORTS, {
      ...options,
      body,
    });
  }

  /**
   * GET /api/v1/reports/export. Ordered newest first, always sent as an
   * attachment (`Content-Disposition` on the response).
   *
   * The response body is a `SavedReport[]` by default and CSV TEXT when
   * `format: "csv"` is passed — the server switches on that query parameter, not
   * on `Accept` — so the declared type is the union of both representations and
   * callers narrow on it. `created_after`/`created_before` are compared as plain
   * strings against the stored timestamp text, so pass full
   * `%Y-%m-%dT%H:%M:%SZ` values; they are neither parsed nor validated.
   */
  export(
    query?: ExportReportsQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<ExportReportsResponse>> {
    return this.client.request<ExportReportsResponse>("GET", REPORTS_EXPORT, {
      ...options,
      query,
    });
  }

  /** GET /api/v1/reports/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<SavedReport>> {
    return this.client.request<SavedReport>("GET", expandPath(REPORT, { id }), {
      ...options,
    });
  }

  /** PATCH /api/v1/reports/{id}. Partial update; omitted fields keep their
   * stored values. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateReportRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<SavedReport>> {
    return this.client.request<SavedReport>(
      "PATCH",
      expandPath(REPORT, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/reports/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(REPORT, { id }), {
      ...options,
    });
  }
}
