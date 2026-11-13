/**
 * Web tooling (Milestone 4): File Watchers, Live Edit, Vite/Webpack aliases in imports,
 * Svelte / Astro / Nuxt / Nx / Prisma / Deno / Bun basics, OpenAPI, GraphQL and the HTTP
 * client extras (cURL, OAuth 2.0 browser login, WebSocket and gRPC requests). The logic is
 * in Rust (`forge-runtools::watchers`, `webforge-lang-web`, `webforge-web-tooling`,
 * `forge-scratches`, `forge-open::live_edit`); these modules render it.
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerMainMenuEntries, action } from "@/features/mainMenu/model";
import { isPluginActive } from "@/state/project";
import "./languages";
import "./fileWatchers";
import "./liveEdit";
import "./imports";
import "./graphql";
import "./openapi";
import "./nx";
import "./frameworks";
import "./httpExtras";
import "./webTooling.css";

const page = (id: string, title: string, order: number, keywords: string[]) =>
  registerSettingsPage({ id, title, group: "Languages & Frameworks", order, keywords, component: () => <SchemaPage pageId={id} /> });

page("lang.graphql", "GraphQL", 61, ["graphql", "schema", "introspection", "endpoint", "graphql.config", "graphqlrc"]);
page("lang.deno", "Deno", 62, ["deno", "deno.json", "deno lsp", "deno task"]);
page("lang.bun", "Bun", 63, ["bun", "bun run", "bun test", "bunfig"]);

registerMainMenuEntries(
  "Tools",
  [
    { type: "group", label: "HTTP Client", items: [action("HttpClient.ImportFromCurl", "Import from cURL..."), action("HttpClient.CopyAsCurl", "Copy as cURL"), action("HttpClient.ClearOAuthTokens", "Clear OAuth Tokens")] },
  ],
  {},
);
registerMainMenuEntries("Tools", [action("GraphQL.Introspect", "Get GraphQL Schema from Endpoint...")], { when: () => isPluginActive("forge.graphql") });
registerMainMenuEntries(
  "Tools",
  [{ type: "group", label: "Prisma", items: [action("Prisma.Generate", "Generate Client"), action("Prisma.MigrateDev", "Migrate Dev...")] }],
  { when: () => isPluginActive("forge.prisma") },
);
