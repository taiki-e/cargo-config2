// SPDX-License-Identifier: Apache-2.0 OR MIT

/*!
Load and resolve [Cargo configuration](https://doc.rust-lang.org/nightly/cargo/reference/config.html).

This library is intended to accurately emulate the actual behavior of Cargo configuration, for example, this supports the following behaviors:

- [Hierarchical structure and merge](https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure)
- [Environment variables](https://doc.rust-lang.org/nightly/cargo/reference/config.html#environment-variables) and [relative paths](https://doc.rust-lang.org/nightly/cargo/reference/config.html#config-relative-paths) resolution.
- `target.<triple>` and `target.<cfg>` resolution.

Supported tables and fields are mainly based on [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)'s use cases, but feel free to submit an issue if you see something missing in your use case.

## Examples

```
# fn main() -> anyhow::Result<()> {
// Read config files hierarchically from the current directory, merge them,
// apply environment variables, and resolve relative paths.
let config = cargo_config2::Config::load()?;
let target = "x86_64-unknown-linux-gnu";
// Resolve target-specific configuration (`target.<triple>` and `target.<cfg>`),
// and returns the resolved rustflags for `target`.
let rustflags = config.rustflags(target)?;
println!("{rustflags:?}");
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
    // Lints that may help when writing public library.
    missing_debug_implementations,
    // missing_docs,
    clippy::alloc_instead_of_core,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    clippy::impl_trait_in_params,
    // clippy::missing_inline_in_public_items,
    // clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
)]
#![allow(clippy::must_use_candidate)]

// Refs:
// - https://doc.rust-lang.org/nightly/cargo/reference/config.html

#[cfg(test)]
#[path = "gen/assert_impl.rs"]
mod assert_impl;
#[path = "gen/is_none.rs"]
mod is_none_impl;

#[macro_use]
mod error;

#[macro_use]
mod process;

mod cfg_expr;
pub mod de;
mod easy;
mod env;
mod merge;
mod resolve;
mod value;
mod walk;

#[doc(no_inline)]
pub use crate::de::{Color, Frequency, RegistriesProtocol, When};
pub use crate::{
    easy::{
        BuildConfig, Config, DocConfig, EnvConfigValue, Flags, FutureIncompatReportConfig,
        NetConfig, PathAndArgs, RegistriesConfigValue, RegistryConfig, StringList, TargetConfig,
        TermConfig, TermProgressConfig,
    },
    error::Error,
    resolve::{CargoVersion, ResolveOptions, RustcVersion, TargetTriple, TargetTripleRef},
    walk::Walk,
};
