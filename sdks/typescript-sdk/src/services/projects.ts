/**
 * Typed projects-service module: projects plus the four sub-resources hanging
 * off them — milestones (which own deliverables), messages, links, and email
 * threads — using the types generated from projects-service/openapi.yaml.
 *
 * This is the only service on the platform with nested resources, and its route
 * layout is worth reading before using the methods, because the shape is not the
 * one the create call suggests:
 *
 * - A sub-resource is CREATED under its parent (`POST
 *   /api/v1/projects/{project_id}/milestones`) but afterwards addressed at its
 *   OWN top-level route (`PATCH /api/v1/milestones/{id}`). So `createMilestone`
 *   takes a project id and `updateMilestone` takes a milestone id; they are not
 *   the same identifier and the API cannot tell you when they are swapped — a
 *   project id passed to `updateMilestone` is simply a 404.
 * - Deliverables repeat that pattern one level down, under a MILESTONE id.
 * - The surfaces are deliberately uneven, and the module mirrors the spec rather
 *   than inventing the missing halves: milestones and deliverables have no
 *   get-by-id, links have no update, messages have no update or delete, and
 *   emails are read-plus-sync only with no direct write.
 *
 * `list` returns a bare array here, not the paginated envelope accounts and
 * contacts return; every response type is derived from its own operation so
 * that difference is carried by the types rather than assumed away.
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
import type { components, operations, paths } from "../generated/projects.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/projects",
  "/api/v1/projects/{id}",
  "/api/v1/projects/{project_id}/milestones",
  "/api/v1/milestones/{id}",
  "/api/v1/milestones/{milestone_id}/deliverables",
  "/api/v1/deliverables/{id}",
  "/api/v1/projects/{project_id}/messages",
  "/api/v1/projects/{project_id}/links",
  "/api/v1/links/{id}",
  "/api/v1/projects/{project_id}/emails",
  "/api/v1/projects/{project_id}/emails/sync",
] as const satisfies readonly (keyof paths)[];

const [
  PROJECTS,
  PROJECT,
  PROJECT_MILESTONES,
  MILESTONE,
  MILESTONE_DELIVERABLES,
  DELIVERABLE,
  PROJECT_MESSAGES,
  PROJECT_LINKS,
  LINK,
  PROJECT_EMAILS,
  PROJECT_EMAILS_SYNC,
] = ROUTES;

/** Fails the build if projects-service/openapi.yaml gains a route this module
 * does not call. The error names the uncovered path. With eleven routes and
 * four sub-resources this is the module most likely to fall behind its spec. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

/** The project listing has no pagination, filter or query string at all today.
 * A spec that grows one fails the build here rather than leaving the SDK unable
 * to send it. */
type _ListDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["listProjects"]>
>;

export type Project = components["schemas"]["Project"];
export type CreateProjectRequest = components["schemas"]["CreateProjectRequest"];
export type UpdateProjectRequest = components["schemas"]["UpdateProjectRequest"];
export type Milestone = components["schemas"]["Milestone"];
export type CreateMilestoneRequest =
  components["schemas"]["CreateMilestoneRequest"];
export type UpdateMilestoneRequest =
  components["schemas"]["UpdateMilestoneRequest"];
export type Deliverable = components["schemas"]["Deliverable"];
export type CreateDeliverableRequest =
  components["schemas"]["CreateDeliverableRequest"];
export type UpdateDeliverableRequest =
  components["schemas"]["UpdateDeliverableRequest"];
export type Message = components["schemas"]["Message"];
export type CreateMessageRequest = components["schemas"]["CreateMessageRequest"];
export type ProjectLink = components["schemas"]["ProjectLink"];
export type CreateProjectLinkRequest =
  components["schemas"]["CreateProjectLinkRequest"];
export type ProjectEmail = components["schemas"]["ProjectEmail"];
export type SyncEmailsRequest = components["schemas"]["SyncEmailsRequest"];

export type ListProjectsResponse = JsonBody<operations["listProjects"], 200>;
export type ListMilestonesResponse = JsonBody<
  operations["listMilestones"],
  200
>;
export type ListDeliverablesResponse = JsonBody<
  operations["listDeliverables"],
  200
>;
export type ListMessagesResponse = JsonBody<operations["listMessages"], 200>;
export type ListLinksResponse = JsonBody<operations["listLinks"], 200>;
export type ListEmailsResponse = JsonBody<operations["listEmails"], 200>;
export type SyncEmailsResponse = JsonBody<operations["syncEmails"], 200>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class ProjectsApi {
  constructor(private readonly client: InfraPortalClient) {}

  // --- Projects -------------------------------------------------------------

  /** GET /api/v1/projects. Newest first, as a bare array with no pagination.
   * Admins see every project; a client sees only their own. */
  list(options?: CallOptions): Promise<ApiResponse<ListProjectsResponse>> {
    return this.client.request<ListProjectsResponse>("GET", PROJECTS, {
      ...options,
    });
  }

  /** POST /api/v1/projects. Admin-only, returns 201. `status` defaults to
   * "active" when omitted or blank. Retried automatically only on 429. */
  create(
    body: CreateProjectRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Project>> {
    return this.client.request<Project>("POST", PROJECTS, {
      ...options,
      body,
    });
  }

  /** GET /api/v1/projects/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<Project>> {
    return this.client.request<Project>("GET", expandPath(PROJECT, { id }), {
      ...options,
    });
  }

  /** PATCH /api/v1/projects/{id}. Admin-only partial update; omitted fields
   * keep their stored values. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateProjectRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Project>> {
    return this.client.request<Project>(
      "PATCH",
      expandPath(PROJECT, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/projects/{id}. Admin-only. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(PROJECT, { id }), {
      ...options,
    });
  }

  // --- Milestones (created under a project, addressed by their own id) -------

  /** GET /api/v1/projects/{project_id}/milestones. */
  listMilestones(
    projectId: string,
    options?: CallOptions,
  ): Promise<ApiResponse<ListMilestonesResponse>> {
    return this.client.request<ListMilestonesResponse>(
      "GET",
      expandPath(PROJECT_MILESTONES, { project_id: projectId }),
      { ...options },
    );
  }

  /** POST /api/v1/projects/{project_id}/milestones. Admin-only, returns 201.
   * Takes the PROJECT id; the milestone it returns carries the id every later
   * call uses. */
  createMilestone(
    projectId: string,
    body: CreateMilestoneRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Milestone>> {
    return this.client.request<Milestone>(
      "POST",
      expandPath(PROJECT_MILESTONES, { project_id: projectId }),
      { ...options, body },
    );
  }

  /** PATCH /api/v1/milestones/{id}. Takes the MILESTONE id, not the project's.
   * Admin-only. Retried automatically only on 429. */
  updateMilestone(
    id: string,
    body: UpdateMilestoneRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Milestone>> {
    return this.client.request<Milestone>(
      "PATCH",
      expandPath(MILESTONE, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/milestones/{id}. Takes the MILESTONE id. Admin-only,
   * resolves with no data on 204. */
  deleteMilestone(
    id: string,
    options?: CallOptions,
  ): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(MILESTONE, { id }), {
      ...options,
    });
  }

  // --- Deliverables (created under a milestone, addressed by their own id) ---

  /** GET /api/v1/milestones/{milestone_id}/deliverables. Takes the MILESTONE
   * id, not the project's. */
  listDeliverables(
    milestoneId: string,
    options?: CallOptions,
  ): Promise<ApiResponse<ListDeliverablesResponse>> {
    return this.client.request<ListDeliverablesResponse>(
      "GET",
      expandPath(MILESTONE_DELIVERABLES, { milestone_id: milestoneId }),
      { ...options },
    );
  }

  /** POST /api/v1/milestones/{milestone_id}/deliverables. Admin-only, returns
   * 201. Takes the MILESTONE id. */
  createDeliverable(
    milestoneId: string,
    body: CreateDeliverableRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Deliverable>> {
    return this.client.request<Deliverable>(
      "POST",
      expandPath(MILESTONE_DELIVERABLES, { milestone_id: milestoneId }),
      { ...options, body },
    );
  }

  /** PATCH /api/v1/deliverables/{id}. Takes the DELIVERABLE id. Admin-only,
   * retried automatically only on 429. */
  updateDeliverable(
    id: string,
    body: UpdateDeliverableRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Deliverable>> {
    return this.client.request<Deliverable>(
      "PATCH",
      expandPath(DELIVERABLE, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/deliverables/{id}. Takes the DELIVERABLE id. Admin-only,
   * resolves with no data on 204. */
  deleteDeliverable(
    id: string,
    options?: CallOptions,
  ): Promise<ApiResponse<void>> {
    return this.client.request<void>(
      "DELETE",
      expandPath(DELIVERABLE, { id }),
      { ...options },
    );
  }

  // --- Messages (create and read only) --------------------------------------

  /** GET /api/v1/projects/{project_id}/messages. */
  listMessages(
    projectId: string,
    options?: CallOptions,
  ): Promise<ApiResponse<ListMessagesResponse>> {
    return this.client.request<ListMessagesResponse>(
      "GET",
      expandPath(PROJECT_MESSAGES, { project_id: projectId }),
      { ...options },
    );
  }

  /** POST /api/v1/projects/{project_id}/messages. Returns 201. Unlike the other
   * writes here, a client (not only an admin) may post to their own project. */
  createMessage(
    projectId: string,
    body: CreateMessageRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Message>> {
    return this.client.request<Message>(
      "POST",
      expandPath(PROJECT_MESSAGES, { project_id: projectId }),
      { ...options, body },
    );
  }

  // --- Links (created under a project, deleted by their own id) --------------

  /** GET /api/v1/projects/{project_id}/links. */
  listLinks(
    projectId: string,
    options?: CallOptions,
  ): Promise<ApiResponse<ListLinksResponse>> {
    return this.client.request<ListLinksResponse>(
      "GET",
      expandPath(PROJECT_LINKS, { project_id: projectId }),
      { ...options },
    );
  }

  /** POST /api/v1/projects/{project_id}/links. Admin-only, returns 201. */
  createLink(
    projectId: string,
    body: CreateProjectLinkRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<ProjectLink>> {
    return this.client.request<ProjectLink>(
      "POST",
      expandPath(PROJECT_LINKS, { project_id: projectId }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/links/{id}. Takes the LINK id. There is no update route for
   * a link: replace it by deleting and re-creating. Resolves with no data
   * on 204. */
  deleteLink(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(LINK, { id }), {
      ...options,
    });
  }

  // --- Email threads (read plus batch sync) ---------------------------------

  /** GET /api/v1/projects/{project_id}/emails. */
  listEmails(
    projectId: string,
    options?: CallOptions,
  ): Promise<ApiResponse<ListEmailsResponse>> {
    return this.client.request<ListEmailsResponse>(
      "GET",
      expandPath(PROJECT_EMAILS, { project_id: projectId }),
      { ...options },
    );
  }

  /**
   * POST /api/v1/projects/{project_id}/emails/sync. Admin-only batch upsert
   * keyed on (project_id, thread_id), answering 200 with the upserted count.
   *
   * Not transactional and not idempotent: the emails are validated and upserted
   * one at a time in request order, so a body that fails validation partway
   * leaves everything before the failure already written and the 400 does not
   * roll it back. Retry only after re-reading what landed.
   */
  syncEmails(
    projectId: string,
    body: SyncEmailsRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<SyncEmailsResponse>> {
    return this.client.request<SyncEmailsResponse>(
      "POST",
      expandPath(PROJECT_EMAILS_SYNC, { project_id: projectId }),
      { ...options, body },
    );
  }
}
