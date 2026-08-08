/**
 * Typed opportunities-service module: list/get/create/update/delete over
 * /api/v1/opportunities, using the types generated from
 * opportunities-service/openapi.yaml.
 *
 * Note the list shape: this service returns a bare `Opportunity[]`, not the
 * paginated envelope accounts and contacts return. `ListOpportunitiesResponse`
 * is derived from the operation's own 200 response so that difference is
 * carried by the types rather than assumed away.
 */
import type {
  ApiResponse,
  InfraPortalClient,
  RequestOptions,
} from "../core/client.js";
import { expandPath } from "../core/routes.js";
import type {
  AssertNoUncoveredRoutes,
  JsonBody,
  ProbePath,
} from "../core/routes.js";
import type {
  components,
  operations,
  paths,
} from "../generated/opportunities.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/opportunities",
  "/api/v1/opportunities/{id}",
] as const satisfies readonly (keyof paths)[];

const [OPPORTUNITIES, OPPORTUNITY] = ROUTES;

/** Fails the build if opportunities-service/openapi.yaml gains a route this
 * module does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

export type Opportunity = components["schemas"]["Opportunity"];
export type CreateOpportunityRequest =
  components["schemas"]["CreateOpportunityRequest"];
export type UpdateOpportunityRequest =
  components["schemas"]["UpdateOpportunityRequest"];
export type ListOpportunitiesResponse = JsonBody<
  operations["listOpportunities"],
  200
>;
export type ListOpportunitiesQuery = NonNullable<
  operations["listOpportunities"]["parameters"]["query"]
>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class OpportunitiesApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/opportunities. Newest first; `owner_id` restricts results to
   * one owner. Returns a bare array, not a paginated envelope. */
  list(
    query?: ListOpportunitiesQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<ListOpportunitiesResponse>> {
    return this.client.request<ListOpportunitiesResponse>(
      "GET",
      OPPORTUNITIES,
      { ...options, query },
    );
  }

  /** GET /api/v1/opportunities/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<Opportunity>> {
    return this.client.request<Opportunity>(
      "GET",
      expandPath(OPPORTUNITY, { id }),
      { ...options },
    );
  }

  /** POST /api/v1/opportunities. Returns 201; `owner_id` is taken from the JWT
   * `sub` claim, not from the body. Retried automatically only on 429. */
  create(
    body: CreateOpportunityRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Opportunity>> {
    return this.client.request<Opportunity>("POST", OPPORTUNITIES, {
      ...options,
      body,
    });
  }

  /** PATCH /api/v1/opportunities/{id}. Partial update; omitted fields keep
   * their stored values. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateOpportunityRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Opportunity>> {
    return this.client.request<Opportunity>(
      "PATCH",
      expandPath(OPPORTUNITY, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/opportunities/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>(
      "DELETE",
      expandPath(OPPORTUNITY, { id }),
      { ...options },
    );
  }
}
