# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org).

<!--
Note: In this file, do not use the hard wrap in the middle of a sentence for compatibility with GitHub comment style markdown rendering.
-->

## [Unreleased]

## [0.1.26] - 2024-04-20

- Fix regression [when buggy rustc_workspace_wrapper is set](https://github.com/cuviper/autocfg/issues/58#issuecomment-2067625980), introduced in 0.1.25.

## [0.1.25] - 2024-04-17

- Respect rustc_wrapper and rustc_workspace_wrapper in `Config::{rustc_version, host_triple}` to match the [Cargo's new behavior](https://github.com/rust-lang/cargo/pull/13659). (Other APIs such as `Config::rustc` are already respecting wrappers.)

## [0.1.24] - 2024-04-09

- Fix bug when merging array fields in config.

## [0.1.23] - 2024-03-29

- Fix `Config::rustc` when both rustc_wrapper and rustc_workspace_wrapper are set.

## [0.1.22] - 2024-03-20

- Implement `From<&PathAndArgs>` for `std::process::Command`.

## [0.1.21] - 2024-03-20

- Add `{RustcVersion,CargoVersion}::major_minor`.

## [0.1.20] - 2024-03-20

- Add `Config::{rustc_version, cargo_version}`.

## [0.1.19] - 2024-02-10

- Update `toml_edit` to 0.22.

## [0.1.18] - 2024-01-25

- Make `home` dependency Windows-only dependency.

## [0.1.17] - 2023-12-16

- Remove dependency on `once_cell`.

## [0.1.16] - 2023-11-17

- Support `target.'cfg(..)'.linker` [that added in Cargo 1.74](https://github.com/rust-lang/cargo/pull/12535).

- Update `toml_edit` to 0.21.

## [0.1.15] - 2023-10-24

- Improve compile time.

## [0.1.14] - 2023-10-18

- Improve compile time.

## [0.1.13] - 2023-10-17

- Improve compatibility with old Cargo.

## [0.1.12] - 2023-09-14

- Improve robustness when new cfgs are added in the future.

- Update `toml` to 0.8.

## [0.1.11] - 2023-09-11

- Remove dependency on `shell-escape`.

## [0.1.10] - 2023-09-08

- Remove dependency on `cfg-expr`.

## [0.1.9] - 2023-08-22

- Recognize unstable `target.cfg(relocation_model = "...")` on nightly.

## [0.1.8] - 2023-07-03

- Fix build error from dependency when built with `-Z minimal-versions`.

## [0.1.7] - 2023-04-05

- Update `cfg-expr` to 0.15.

## [0.1.6] - 2023-03-07

- Implement the `[registries]` and `[registry]` tables. ([#8](https://github.com/taiki-e/cargo-config2/pull/8), thanks @yottalogical)

## [0.1.5] - 2023-02-23

- Fix handling of empty string rustc wrapper envs. ([#7](https://github.com/taiki-e/cargo-config2/pull/7), thanks @tofay)

## [0.1.4] - 2023-01-28

- Update `cfg-expr` to 0.14.

- Update `toml` to 0.7.

## [0.1.3] - 2023-01-24

- Migrate to `toml` 0.6. ([#6](https://github.com/taiki-e/cargo-config2/pull/6))

## [0.1.2] - 2023-01-10

- Improve error messages.

- Add `Config::cargo` method.

- Documentation improvements.

## [0.1.1] - 2023-01-09

- Fix `serde::Serialize` impl of `Config` after target resolved.

## [0.1.0] - 2023-01-09

Initial release

[Unreleased]: https://github.com/taiki-e/cargo-config2/compare/v0.1.26...HEAD
[0.1.26]: https://github.com/taiki-e/cargo-config2/compare/v0.1.25...v0.1.26
[0.1.25]: https://github.com/taiki-e/cargo-config2/compare/v0.1.24...v0.1.25
[0.1.24]: https://github.com/taiki-e/cargo-config2/compare/v0.1.23...v0.1.24
[0.1.23]: https://github.com/taiki-e/cargo-config2/compare/v0.1.22...v0.1.23
[0.1.22]: https://github.com/taiki-e/cargo-config2/compare/v0.1.21...v0.1.22
[0.1.21]: https://github.com/taiki-e/cargo-config2/compare/v0.1.20...v0.1.21
[0.1.20]: https://github.com/taiki-e/cargo-config2/compare/v0.1.19...v0.1.20
[0.1.19]: https://github.com/taiki-e/cargo-config2/compare/v0.1.18...v0.1.19
[0.1.18]: https://github.com/taiki-e/cargo-config2/compare/v0.1.17...v0.1.18
[0.1.17]: https://github.com/taiki-e/cargo-config2/compare/v0.1.16...v0.1.17
[0.1.16]: https://github.com/taiki-e/cargo-config2/compare/v0.1.15...v0.1.16
[0.1.15]: https://github.com/taiki-e/cargo-config2/compare/v0.1.14...v0.1.15
[0.1.14]: https://github.com/taiki-e/cargo-config2/compare/v0.1.13...v0.1.14
[0.1.13]: https://github.com/taiki-e/cargo-config2/compare/v0.1.12...v0.1.13
[0.1.12]: https://github.com/taiki-e/cargo-config2/compare/v0.1.11...v0.1.12
[0.1.11]: https://github.com/taiki-e/cargo-config2/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/taiki-e/cargo-config2/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/taiki-e/cargo-config2/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/taiki-e/cargo-config2/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/taiki-e/cargo-config2/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/taiki-e/cargo-config2/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/taiki-e/cargo-config2/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/taiki-e/cargo-config2/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/taiki-e/cargo-config2/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/taiki-e/cargo-config2/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/taiki-e/cargo-config2/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/taiki-e/cargo-config2/releases/tag/v0.1.0
