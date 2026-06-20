# jj-version-macro

This crate contains the procedural macro implementation for
[`jj-version`](https://crates.io/crates/jj-version).

You should usually depend on `jj-version` instead of this crate directly:

```toml
[dependencies]
jj-version = "0.1"
```

Then use the re-exported macro:

```rust
use jj_version::jj_version;

const VERSION: &str = jj_version!(fallback = "unknown");
```

The `jj-version-macro` crate is published separately because Rust procedural
macros must live in a `proc-macro` crate.
