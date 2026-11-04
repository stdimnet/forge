//! JavaForge's Minecraft plugin support, like IntelliJ's Minecraft Development plugin. Pure
//! Rust (no file system or processes), so the desktop backend and the browser mode
//! (WebAssembly) share it.
//!
//! - [`versions`]: the pinned table of Minecraft versions, their Java releases and each
//!   platform's API repositories and coordinates; parsing Paper's `maven-metadata.xml` for
//!   "Refresh versions" ([`net`], feature `net`, only on user action).
//! - [`generate`]: the "Minecraft Plugin" New Project generator (Paper, Purpur, Folia,
//!   Spigot, Bukkit, Velocity, BungeeCord; Gradle Kotlin / Groovy DSL or Maven; Java or
//!   Kotlin; run-paper and Shadow), with the source templates in [`sources`].
//! - [`detect`]: a Minecraft plugin project from its build files.
//! - [`descriptor`]: the `main` class of `plugin.yml`, `paper-plugin.yml`, `bungee.yml` and
//!   `velocity-plugin.json`: inspection and navigation.
//! - [`markers`]: gutter markers for event handlers and the plugin's main class.

pub mod descriptor;
pub mod detect;
pub mod generate;
pub mod markers;
#[cfg(feature = "net")]
pub mod net;
pub mod sources;
pub mod versions;

pub use detect::{detect, Detected};
pub use generate::{generate, GeneratedFile, Options};
pub use versions::Platform;
