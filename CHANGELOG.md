# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org).

<!--
Note: In this file, do not use the hard wrap in the middle of a sentence for compatibility with GitHub comment style markdown rendering.
-->

## [Unreleased]

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

[Unreleased]: https://github.com/taiki-e/cargo-config2/compare/v0.1.9...HEAD
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
