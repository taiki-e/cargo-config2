# cargo-config2

[![crates.io](https://img.shields.io/crates/v/cargo-config2?style=flat-square&logo=rust)](https://crates.io/crates/cargo-config2)
[![docs.rs](https://img.shields.io/badge/docs.rs-cargo--config2-blue?style=flat-square&logo=docs.rs)](https://docs.rs/cargo-config2)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)
[![rustc](https://img.shields.io/badge/rustc-1.58+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![build status](https://img.shields.io/github/actions/workflow/status/taiki-e/cargo-config2/ci.yml?branch=main&style=flat-square&logo=github)](https://github.com/taiki-e/cargo-config2/actions)

Structured access to [Cargo configuration](https://doc.rust-lang.org/nightly/cargo/reference/config.html).

This library is intended to accurately emulate the actual behavior of Cargo configuration, for example, this supports the following behaviors:

- [Hierarchical structure and merge](https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure)
- Environment variables resolution.
- `target.<triple>` and `target.<cfg>` resolution.

Supported tables and fields are mainly based on [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)'s use cases, but feel free to submit an issue if you see something missing in your use case.

## Examples

```rust
// Read config files hierarchically from the current directory and merges them.
let mut config = cargo_config2::Config::load()?;
// Apply environment variables and resolves target-specific configuration
// (`target.<triple>` and `target.<cfg>`).
let target = "x86_64-unknown-linux-gnu";
config.resolve(target)?;
// Display resolved rustflags for `target`.
println!("{:?}", config.target[target].rustflags);
```

See also the [`get` example](https://github.com/taiki-e/cargo-config2/blob/HEAD/examples/get.rs) that partial re-implementation of `cargo config get` using cargo-config2.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
