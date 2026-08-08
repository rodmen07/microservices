/**
 * Typed integrations-service module: list/get/create/update/delete over
 * /api/v1/integrations/connections, using the types generated from
 * integrations-service/openapi.yaml.
 *
 * Note the two-segment resource path, which is why the route literals are
 * pinned to the spec rather than composed from the service name.
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
import type {
  components,
  operations,
  paths,
} from "../generated/integrations.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/integrations/connections",
  "/api/v1/integrations/connections/{id}",
] as const satisfies readonly (keyof paths)[];

const [CONNECTIONS, CONNECTION] = ROUTES;

/** Fails the build if integrations-service/openapi.yaml gains a route this
 * module does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

/** Fails the build if listConnections gains query parameters `list()` cannot send. */
type _ListDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["listConnections"]>
>;

export type IntegrationConnection =
  components["schemas"]["IntegrationConnection"];
export type CreateConnectionRequest =
  components["schemas"]["CreateConnectionRequest"];
export type UpdateConnectionRequest =
  components["schemas"]["UpdateConnectionRequest"];
export type ListConnectionsResponse = JsonBody<
  operations["listConnections"],
  200
>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class IntegrationsApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/integrations/connections. Takes no parameters — the spec
   * declares none. */
  list(options?: CallOptions): Promise<ApiResponse<ListConnectionsResponse>> {
    return this.client.request<ListConnectionsResponse>("GET", CONNECTIONS, {
      ...options,
    });
  }

  /** GET /api/v1/integrations/connections/{id}. Throws ApiError NOT_FOUND (404)
   * when absent. */
  get(
    id: string,
    options?: CallOptions,
  ): Promise<ApiResponse<IntegrationConnection>> {
    return this.client.request<IntegrationConnection>(
      "GET",
      expandPath(CONNECTION, { id }),
      { ...options },
    );
  }

  /** POST /api/v1/integrations/connections. Returns 201 with the created
   * connection. Retried automatically only on 429. */
  create(
    body: CreateConnectionRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<IntegrationConnection>> {
    return this.client.request<IntegrationConnection>("POST", CONNECTIONS, {
      ...options,
      body,
    });
  }

  /** PATCH /api/v1/integrations/connections/{id}. Partial update; omitted
   * fields keep their stored values. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateConnectionRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<IntegrationConnection>> {
    return this.client.request<IntegrationConnection>(
      "PATCH",
      expandPath(CONNECTION, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/integrations/connections/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>(
      "DELETE",
      expandPath(CONNECTION, { id }),
      { ...options },
    );
  }
}
