/**
 * Typed activities-service module: list/get/create/update/delete over
 * /api/v1/activities, using the types generated from
 * activities-service/openapi.yaml.
 *
 * `list()` deliberately takes NO argument: the spec declares no parameters at
 * all for `listActivities`, so there is nothing this client could legally send.
 * `_ListDeclaresNoParameters` pins that, so a spec that later grows a filter
 * fails the build here instead of leaving the SDK silently unable to express it.
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
import type { components, operations, paths } from "../generated/activities.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/activities",
  "/api/v1/activities/{id}",
] as const satisfies readonly (keyof paths)[];

const [ACTIVITIES, ACTIVITY] = ROUTES;

/** Fails the build if activities-service/openapi.yaml gains a route this module
 * does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

/** Fails the build if listActivities gains query parameters `list()` cannot send. */
type _ListDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["listActivities"]>
>;

export type Activity = components["schemas"]["Activity"];
export type CreateActivityRequest =
  components["schemas"]["CreateActivityRequest"];
export type UpdateActivityRequest =
  components["schemas"]["UpdateActivityRequest"];
export type ListActivitiesResponse = JsonBody<
  operations["listActivities"],
  200
>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class ActivitiesApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/activities. Takes no parameters — the spec declares none. */
  list(options?: CallOptions): Promise<ApiResponse<ListActivitiesResponse>> {
    return this.client.request<ListActivitiesResponse>("GET", ACTIVITIES, {
      ...options,
    });
  }

  /** GET /api/v1/activities/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<Activity>> {
    return this.client.request<Activity>("GET", expandPath(ACTIVITY, { id }), {
      ...options,
    });
  }

  /** POST /api/v1/activities. Returns 201 with the created activity. Retried
   * automatically only on 429. */
  create(
    body: CreateActivityRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Activity>> {
    return this.client.request<Activity>("POST", ACTIVITIES, {
      ...options,
      body,
    });
  }

  /** PATCH /api/v1/activities/{id}. Partial update; omitted fields keep their
   * stored values. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateActivityRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Activity>> {
    return this.client.request<Activity>(
      "PATCH",
      expandPath(ACTIVITY, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/activities/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(ACTIVITY, { id }), {
      ...options,
    });
  }
}
