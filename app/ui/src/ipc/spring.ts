/**
 * Spring Boot and Jakarta EE (Rust: `packs/javaforge/fw-spring`, `commands/spring.rs`):
 * the project model (beans, endpoints, JPA entities, configuration properties), gutter
 * markers, configuration file assistance, HTTP Client requests and the run dashboard.
 * Offsets are UTF-16; locations are 0-based.
 */
import { invoke, listen } from "./index";
import type { SchemaCompletion } from "./jsonSchema";
import type { ServiceDetails, ServiceNode } from "./services";

export interface SpringLocation {
  path: string;
  line: number;
  column: number;
}

export interface EndpointParam {
  name: string;
  kind: "path" | "query" | "header" | "body";
  type: string;
}

/** A row of the Endpoints tool window. */
export interface EndpointRow {
  /** `GET`, `POST`, … or `ANY`. */
  method: string;
  path: string;
  /** `OrderController.find`. */
  handler: string;
  /** `spring`, `jaxrs` or `runtime` (only the running application knows it). */
  framework: string;
  location?: SpringLocation;
  produces?: string[];
  consumes?: string[];
  params?: EndpointParam[];
  /** The running application serves it (actuator mappings). */
  live?: boolean;
}

export interface BeanRow {
  name: string;
  type: string;
  /** `component`, `service`, `repository`, `controller`, `restController`, `configuration`,
   * `application`, `bean`, `dataRepository`, `cdi`, `producer`, `ejb`, `resource` or
   * `runtime` (from the running application). */
  kind: string;
  framework: "spring" | "cdi";
  scope?: string;
  profiles?: string[];
  location?: SpringLocation;
  usages: number;
  dependencies?: string[];
  live?: boolean;
}

export interface EntityField {
  name: string;
  type: string;
  column?: string;
  id?: boolean;
  generated?: boolean;
  relation?: string;
  target?: string;
  line: number;
}

export interface Entity {
  name: string;
  class: string;
  kind: "entity" | "embeddable" | "mappedSuperclass";
  table?: string;
  location: SpringLocation;
  fields: EntityField[];
  repositories?: string[];
  namespace?: "jakarta" | "javax";
}

export interface ConfigClass {
  prefix: string;
  class: string;
  location: SpringLocation;
  keys: string[];
}

export interface SpringApplication {
  class: string;
  name: string;
  mainClass: string;
  kotlin?: boolean;
  location: SpringLocation;
  kind: "spring" | "jaxrs";
}

export interface SpringApi {
  id: string;
  name: string;
  namespace: "jakarta" | "javax" | "spring";
  files: number;
}

export interface ConfigFileInfo {
  path: string;
  format: "properties" | "yaml";
  profile?: string;
  keys: number;
}

export interface SpringOverview {
  spring: boolean;
  jakarta: boolean;
  beans: BeanRow[];
  endpoints: EndpointRow[];
  entities: Entity[];
  configClasses: ConfigClass[];
  applications: SpringApplication[];
  apis: SpringApi[];
  profiles: string[];
  configFiles: ConfigFileInfo[];
  metadata: { properties: number; sources: string[]; problems: string[] };
  /** The running application whose actuator enriched the rows. */
  runtime?: { name: string; url?: string; beans: boolean; mappings: boolean };
}

export interface SpringTarget extends SpringLocation {
  preview: string;
}

export interface SpringMarker {
  line: number;
  kind: "bean" | "injection" | "endpoint" | "entity" | "repository" | "configProperties";
  title: string;
  targets: SpringTarget[];
  /** `endpoint`: indices into the model's endpoints (static ones). */
  endpoints?: number[];
}

export interface ConfigProblem {
  from: number;
  to: number;
  severity: "error" | "warning" | "weak";
  message: string;
  code: "unknownProperty" | "deprecatedProperty" | "invalidValue";
  replacement?: string;
  /** The quick fix: the text replacing `from..to`. */
  fix?: string;
}

export interface ConfigHover {
  from: number;
  to: number;
  markdown: string;
}

export type SpringCompletion = SchemaCompletion & { incomplete: boolean };

export const DASHBOARD_EVENT = "spring://dashboard";

export const springApi = {
  model: (refresh = false) => invoke<SpringOverview>("spring_model", { refresh }),
  gutter: (path: string, content: string) => invoke<SpringMarker[]>("spring_gutter", { path, content }),
  complete: (path: string, content: string, offset: number) => invoke<SpringCompletion | null>("spring_config_complete", { path, content, offset }),
  inspect: (path: string, content: string) => invoke<ConfigProblem[]>("spring_config_inspect", { path, content }),
  hover: (path: string, content: string, offset: number) => invoke<ConfigHover | null>("spring_config_hover", { path, content, offset }),
  goto: (path: string, content: string, offset: number) => invoke<SpringLocation[]>("spring_config_goto", { path, content, offset }),
  httpRequest: (endpoint: EndpointRow) => invoke<string>("spring_http_request", { endpoint }),
  dashboard: () => invoke<ServiceNode[]>("spring_dashboard"),
  dashboardDetails: (nodeId: string) => invoke<ServiceDetails | null>("spring_dashboard_details", { nodeId }),
  onDashboard: (cb: () => void) => listen<unknown>(DASHBOARD_EVENT, () => cb()),
};

/** The Spring Boot run configuration type. */
export const SPRING_BOOT = "spring-boot";
