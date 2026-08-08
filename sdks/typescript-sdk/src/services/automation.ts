/**
 * Typed automation-service module: list/get/create/update/delete over
 * /api/v1/workflows, using the types generated from
 * automation-service/openapi.yaml.
 *
 * Note the route does NOT follow the service name: automation-service serves
 * `/api/v1/workflows` (and its CI database is called `workflows` for the same
 * reason). The route tuple is pinned to the spec, so that mismatch is checked
 * rather than remembered.
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
import type { components, operations, paths } from "../generated/automation.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/workflows",
  "/api/v1/workflows/{id}",
] as const satisfies readonly (keyof paths)[];

const [WORKFLOWS, WORKFLOW] = ROUTES;

/** Fails the build if automation-service/openapi.yaml gains a route this module
 * does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

/** Fails the build if listWorkflows gains query parameters `list()` cannot send. */
type _ListDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["listWorkflows"]>
>;

export type Workflow = components["schemas"]["Workflow"];
export type CreateWorkflowRequest =
  components["schemas"]["CreateWorkflowRequest"];
export type UpdateWorkflowRequest =
  components["schemas"]["UpdateWorkflowRequest"];
export type ListWorkflowsResponse = JsonBody<operations["listWorkflows"], 200>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class AutomationApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/workflows. Takes no parameters — the spec declares none. */
  list(options?: CallOptions): Promise<ApiResponse<ListWorkflowsResponse>> {
    return this.client.request<ListWorkflowsResponse>("GET", WORKFLOWS, {
      ...options,
    });
  }

  /** GET /api/v1/workflows/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<Workflow>> {
    return this.client.request<Workflow>("GET", expandPath(WORKFLOW, { id }), {
      ...options,
    });
  }

  /** POST /api/v1/workflows. Returns 201 with the created workflow. Retried
   * automatically only on 429. */
  create(
    body: CreateWorkflowRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Workflow>> {
    return this.client.request<Workflow>("POST", WORKFLOWS, {
      ...options,
      body,
    });
  }

  /** PATCH /api/v1/workflows/{id}. Partial update; omitted fields keep their
   * stored values. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateWorkflowRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Workflow>> {
    return this.client.request<Workflow>(
      "PATCH",
      expandPath(WORKFLOW, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/workflows/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(WORKFLOW, { id }), {
      ...options,
    });
  }
}
