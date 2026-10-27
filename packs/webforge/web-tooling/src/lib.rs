//! WebForge web tooling (pure, so browser mode runs it as WebAssembly):
//!
//! - [`openapi`]: OpenAPI 3.x / Swagger 2.0 documents: detection, endpoints, the native
//!   preview model and "Generate HTTP Request".
//! - [`graphql`]: GraphQL schemas (SDL files or introspection results), validation of
//!   queries against them and completion, and `graphql.config.*` files.
//! - [`nx`]: Nx workspaces: projects and targets from `nx.json`, `project.json` and
//!   `package.json`, and the project graph.
//! - [`nuxt`]: Nuxt pages (file-based routes) and auto-imported components.
//! - [`deno`]: tasks of `deno.json` / `deno.jsonc`.

pub mod openapi;
pub mod graphql;
pub mod deno;
pub mod nuxt;
pub mod nx;
