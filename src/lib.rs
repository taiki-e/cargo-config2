/*!
Structured access to [Cargo configuration](https://doc.rust-lang.org/nightly/cargo/reference/config.html).

This library is intended to accurately emulate the actual behavior of Cargo configuration, for example, this supports the following behaviors:

- [Hierarchical structure and merge](https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure)
- Environment variables resolution.
- `target.<triple>` and `target.<cfg>` resolution.

Supported tables and fields are mainly based on [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)'s use cases, but feel free to submit an issue if you see something missing in your use case.

## Examples

```no_run
# fn main() -> anyhow::Result<()> {
// Read config files hierarchically from the current directory and merges them.
let config = cargo_config2::Config::load()?;
// Apply environment variables and resolves target-specific configuration
// (`target.<triple>` and `target.<cfg>`).
let target = "x86_64-unknown-linux-gnu";
// Display resolved rustflags for `target`.
println!("{:?}", config.rustflags(target)?);
# Ok(()) }
```

See also the [`get` example](https://github.com/taiki-e/cargo-config2/blob/HEAD/examples/get.rs) that partial re-implementation of `cargo config get` using cargo-config2.
*/

#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms, single_use_lifetimes),
        allow(dead_code, unused_variables)
    )
))]
#![forbid(unsafe_code)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    single_use_lifetimes,
    unreachable_pub
)]
#![warn(
    clippy::pedantic,
    // lints for public library
    // clippy::alloc_instead_of_core,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    // clippy::std_instead_of_alloc,
    // clippy::std_instead_of_core,
)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::single_match_else,
    clippy::single_match,
    clippy::too_many_lines
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Refs:
// - https://doc.rust-lang.org/nightly/cargo/reference/config.html

#[cfg(test)]
#[path = "gen/assert_impl.rs"]
mod assert_impl;
#[path = "gen/is_none.rs"]
mod is_none_impl;

#[macro_use]
mod process;

pub mod de;
mod easy;
mod env;
mod merge;
mod resolve;
mod value;
mod walk;

use anyhow::Result;

#[doc(no_inline)]
pub use crate::de::{Color, Frequency, When};
pub use crate::{
    easy::{
        BuildConfig, Config, DocConfig, EnvConfigValue, Flags, FutureIncompatReportConfig,
        NetConfig, PathAndArgs, StringList, TargetConfig, TermConfig, TermProgressConfig,
    },
    resolve::{ResolveOptions, TargetTriple, TargetTripleRef},
    walk::Walk,
};
