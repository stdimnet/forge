//! Code With Me (`forge.codewithme`): a host invites guests into the project open in their
//! Forge; everyone edits the same files at the same time and sees the others' carets. The
//! plan, the threat model and the stages are in `docs/plans/code-with-me.md`.
//!
//! Pure modules (no tokio, no network; also built for WebAssembly without `runtime`):
//! - [`link`]: session links (`forge://cwm/join?…#…`) and their `cwm1:` text form.
//! - [`permissions`]: permission levels and the [`permissions::Policy`] that decides every
//!   guest request on the host.
//! - [`protocol`]: the messages between guest and host and their framing.
//! - [`verify`]: the join proof and the verification code both sides show.
//! - [`excludes`]: secret files that guests never see.
//! - [`presence`]: participants and their colors.
//! - [`relay`]: relay options (self-hosted relay, public relays, LAN discovery) and the
//!   connection path (direct or relayed).
//! - [`secrets`]: the relay token in the OS keychain.
//! - [`textdiff`]: minimal UTF-16 edits between two texts (reloading shared files from disk).
//!
//! With `runtime` (the default):
//! - [`docs`]: the host's authoritative `yrs` copy of each shared file.
//! - [`host`]: the host's session: admission, shared documents, saving, disk changes.
//! - [`guest`]: a guest's session, forwarding between its editor and the host.
//! - [`transport`]: the [`transport::Transport`] trait and the in-memory transport for tests.

pub mod excludes;
pub mod link;
pub mod permissions;
pub mod presence;
pub mod protocol;
pub mod relay;
pub mod secrets;
pub mod textdiff;
pub mod verify;

#[cfg(feature = "runtime")]
pub mod docs;
#[cfg(feature = "runtime")]
pub mod guest;
#[cfg(feature = "runtime")]
pub mod host;
#[cfg(feature = "runtime")]
pub mod transport;

/// Errors of the pure modules. Messages are worded for the UI.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A session link that cannot be used.
    #[error("Invalid session link: {0}")]
    InvalidLink(String),
    /// Bytes on the wire that are not a valid message.
    #[error("Invalid message: {0}")]
    Protocol(String),
    /// A pattern in the excluded files setting.
    #[error("Invalid exclude pattern \"{0}\"")]
    InvalidPattern(String),
    /// The relay server setting or the relay token.
    #[error("Invalid relay server: {0}")]
    InvalidRelay(String),
}

pub type Result<T> = std::result::Result<T, Error>;
