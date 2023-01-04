#![allow(clippy::drop_non_drop)]

use std::{hint::black_box, path::Path};

use cargo_config2::ResolveContext;
use criterion::{criterion_group, criterion_main, Criterion};
use fs_err as fs;

fn fixtures_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures"))
}

fn reference(c: &mut Criterion) {
    let mut g = c.benchmark_group("reference");
    let dir = &fixtures_path().join("reference");
    let config_path = &dir.join(".cargo/config.toml");
    let buf = &black_box(fs::read(config_path).unwrap());
    g.bench_function("parse_toml_rs", |b| {
        b.iter(|| black_box(toml::from_slice::<cargo_config2::de::Config>(buf).unwrap()));
    });
    g.bench_function("parse_toml_edit", |b| {
        b.iter(|| {
            black_box(toml_edit::easy::from_slice::<cargo_config2::de::Config>(buf).unwrap())
        });
    });
    g.bench_function("apply_env_no_env", |b| {
        let config = &black_box(cargo_config2::de::Config::default());
        let cx = &mut black_box(ResolveContext::no_env());
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
            ("CARGO_TERM_QUIET", "false"),
            ("CARGO_TERM_VERBOSE", "false"),
            ("CARGO_TERM_COLOR", "auto"),
            ("CARGO_TERM_PROGRESS_WHEN", "auto"),
            ("CARGO_TERM_PROGRESS_WIDTH", "100"),
        ];
        let cx = &mut black_box(ResolveContext::with_env(env_list));
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
