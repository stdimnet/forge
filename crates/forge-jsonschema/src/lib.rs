//! JSON Schema support for JSON, JSON5/JSONC and YAML files, like PHPStorm's: validation
//! with PHPStorm's messages, completion of property names and values, documentation from
//! `description` / `markdownDescription`, Go to Declaration into the schema, and the schema
//! mappings (built-in ones for composer.json, package.json, Compose, CI files, ..., user
//! mappings, `$schema` in the file and the SchemaStore catalog).
//!
//! Everything here is pure (it also builds to WebAssembly for browser mode). Loading schema
//! files is up to the caller: [`Registry::missing`] lists the documents `$ref`s point to
//! that are not loaded yet. The `remote` feature adds [`remote::RemoteCache`] for schema
//! downloads into the user cache dir.

pub mod json;
pub mod text;
pub mod tree;
pub mod yaml;
