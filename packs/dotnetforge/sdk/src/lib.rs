//! DotNetForge's machine side: the .NET host and SDKs ([`detect`]), netcoredbg and the
//! Roslyn language server ([`tools`], downloads in `forge_langservers::managed`), and the
//! NuGet feed client ([`nuget`]).

pub mod detect;
pub mod nuget;
pub mod tools;

pub use detect::{detect, DetectEnv, DotnetHost, DotnetStatus};
pub use nuget::{NuGetClient, PackageUpdate};
pub use tools::{find_netcoredbg, find_roslyn, ToolLocation};
