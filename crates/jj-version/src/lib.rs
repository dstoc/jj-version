//! Embed Jujutsu version information in your code at compile time.
//!
//! ```
//! use jj_version::jj_version;
//!
//! const VERSION: &str = jj_version!(
//!     fallback = env!("CARGO_PKG_VERSION"),
//! );
//! ```
//!
//! When Jujutsu metadata is available, the version string is similar to
//! `git describe --tags --always`:
//!
//! - `v1.2.3` when the effective revision is exactly tagged
//! - `v1.2.3-4-gabc123def456` when it is ahead of the nearest tag
//! - `abc123def456` when no tag is reachable
//!
//! If `jj` is unavailable, the current directory is not a Jujutsu repository,
//! or version resolution fails, the macro expands to the fallback expression
//! unchanged.
//!
//! These macros do not depend on `jj-lib`; they invoke the `jj` binary directly.
//! You must have `jj` installed somewhere in `PATH` to resolve Jujutsu metadata.
//!
//! Unlike Git-oriented version macros, `jj-version` does not emit a dirty suffix.
//! In Jujutsu, the working copy is represented as a commit, so the effective
//! revision is either the non-empty working-copy commit or, if `@` is empty, its
//! parent.
//!
//! The macro invokes `jj` with `--ignore-working-copy` and never snapshots or
//! mutates the repository. Unsnapshotted filesystem changes may therefore not be
//! reflected in the generated version string.

pub use jj_version_macro::jj_version;
