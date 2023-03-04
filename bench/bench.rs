#![allow(clippy::drop_non_drop)]

use std::{collections::HashMap, hint::black_box, path::Path};

use cargo_config2::{PathAndArgs, ResolveOptions};
use criterion::{criterion_group, criterion_main, Criterion};

fn fixtures_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures"))
}

fn test_options() -> ResolveOptions {
    ResolveOptions::default()
        .env(HashMap::<String, String>::default())
        .cargo_home(None)
        .rustc(PathAndArgs::new("rustc"))
}

fn reference(c: &mut Criterion) {
    let mut g = c.benchmark_group("reference");
    let dir = &fixtures_path().join("reference");
    g.bench_function("load_config_easy", |b| {
        b.iter(|| {
            let config = cargo_config2::Config::load_with_options(dir, test_options()).unwrap();
            black_box(config)
        });
    });
    g.bench_function("apply_env_no_env", |b| {
        let config = &black_box(cargo_config2::de::Config::default());
        let cx = &mut black_box(test_options().into_context());
        b.iter(|| {
            let mut config = black_box(config.clone());
            config.apply_env(cx).unwrap();
            black_box(config)
        });
    });
    g.bench_function("apply_env_full_env", |b| {
        let config = &black_box(cargo_config2::de::Config::default());
        // NB: sync with test in src/resolve.rs
        let env_list = [
            ("CARGO_BUILD_JOBS", "-1"),
            ("RUSTC", "rustc"),
            ("CARGO_BUILD_RUSTC", "rustc"),
            ("RUSTC_WRAPPER", "rustc_wrapper"),
            ("CARGO_BUILD_RUSTC_WRAPPER", "rustc_wrapper"),
            ("RUSTC_WORKSPACE_WRAPPER", "rustc_workspace_wrapper"),
            ("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "rustc_workspace_wrapper"),
            ("RUSTDOC", "rustdoc"),
            ("CARGO_BUILD_RUSTDOC", "rustdoc"),
            ("CARGO_BUILD_TARGET", "triple"),
            ("CARGO_TARGET_DIR", "target"),
            ("CARGO_BUILD_TARGET_DIR", "target"),
            ("CARGO_ENCODED_RUSTFLAGS", "1"),
            ("RUSTFLAGS", "1"),
            ("CARGO_BUILD_RUSTFLAGS", "1"),
            ("CARGO_ENCODED_RUSTDOCFLAGS", "1"),
            ("RUSTDOCFLAGS", "1"),
            ("CARGO_BUILD_RUSTDOCFLAGS", "1"),
            ("CARGO_INCREMENTAL", "false"),
            ("CARGO_BUILD_INCREMENTAL", "1"),
            ("CARGO_BUILD_DEP_INFO_BASEDIR", "1"),
            ("BROWSER", "1"),
            ("CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY", "always"),
            ("CARGO_NET_RETRY", "1"),
            ("CARGO_NET_GIT_FETCH_WITH_CLI", "false"),
            ("CARGO_NET_OFFLINE", "false"),
            ("CARGO_REGISTRIES_crates-io_INDEX", "https://github.com/rust-lang/crates.io-index"),
            ("CARGO_REGISTRIES_crates-io_TOKEN", "00000000000000000000000000000000000"),
            ("CARGO_REGISTRY_DEFAULT", "crates.io"),
            ("CARGO_REGISTRY_TOKEN", "00000000000000000000000000000000000"),
            ("CARGO_TERM_QUIET", "false"),
            ("CARGO_TERM_VERBOSE", "false"),
            ("CARGO_TERM_COLOR", "auto"),
            ("CARGO_TERM_PROGRESS_WHEN", "auto"),
            ("CARGO_TERM_PROGRESS_WIDTH", "100"),
        ];
        let cx = &mut black_box(
            ResolveOptions::default()
                .env(env_list)
                .cargo_home(None)
                .rustc(PathAndArgs::new("rustc"))
                .into_context(),
        );
        b.iter(|| {
            let mut config = black_box(config.clone());
            config.apply_env(cx).unwrap();
            black_box(config)
        });
    });
    // let config = &toml::from_slice::<cargo_config2::de::Config>(buf).unwrap();
}

criterion_group!(benches, reference);
criterion_main!(benches);
