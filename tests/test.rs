use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context as _, Result};
use cargo_config2::*;
use toml_crate as toml;

fn fixtures_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

fn assert_reference_example(de: fn(&Path) -> Result<Config>) {
    let base_config = &de(&fixtures_path().join("reference")).unwrap();
    let mut config = base_config.clone();

    // [alias]
    for (k, v) in &config.alias {
        match k.as_str() {
            "b" => assert_eq!(*v, "build".into()),
            "c" => assert_eq!(*v, "check".into()),
            "t" => assert_eq!(*v, "test".into()),
            "r" => assert_eq!(*v, "run".into()),
            "rr" => assert_eq!(*v, "run --release".into()),
            "recursive_example" => assert_eq!(*v, "rr --example recursions".into()),
            "space_example" => {
                assert_eq!(*v, ["run", "--release", "--", "\"command list\""].into())
            }
            _ => panic!("unexpected alias: name={k}, value={v:?}"),
        }
    }

    // [build]
    assert_eq!(config.build.rustc, Some("rustc".into()));
    assert_eq!(config.build.rustc_wrapper, Some("…".into()));
    assert_eq!(config.build.rustc_workspace_wrapper, Some("…".into()));
    assert_eq!(config.build.rustdoc, Some("rustdoc".into()));
    assert_eq!(config.build.target, Some("triple".into()));
    assert_eq!(config.build.target_dir, Some("target".into()));
    assert_eq!(config.build.rustflags, Some(["…", "…"].into()));
    assert_eq!(config.build.rustdocflags, Some(["…", "…"].into()));
    assert_eq!(config.build.incremental, Some(true));
    assert_eq!(config.build.dep_info_basedir, Some("…".into()));

    // [doc]
    assert_eq!(config.doc.browser, Some("chromium".into()));

    // [env]
    assert_eq!(config.env["ENV_VAR_NAME"], Env::Value("value".into()));
    assert_eq!(config.env["ENV_VAR_NAME_2"], Env::Table {
        value: "value".into(),
        force: Some(true),
        relative: None
    });
    assert_eq!(config.env["ENV_VAR_NAME_3"], Env::Table {
        value: "relative/path".into(),
        force: None,
        relative: Some(true),
    });

    // [future-incompat-report]
    assert_eq!(config.future_incompat_report.frequency, Some(Frequency::Always));

    // TODO
    // [cargo-new]

    // TODO
    // [http]

    // TODO
    // [install]

    // [net]
    assert_eq!(config.net.retry, Some(2));
    assert_eq!(config.net.git_fetch_with_cli, Some(true));
    assert_eq!(config.net.offline, Some(true));

    // TODO
    // [patch.<registry>]
    // [profile.<name>]
    // [registries.<name>]
    // [registry]
    // [source.<name>]

    // [target.<triple>]
    assert_eq!(config.target["x86_64-unknown-linux-gnu"].linker, Some("b".into()));
    assert_eq!(config.target["x86_64-unknown-linux-gnu"].runner, Some("b".into()));
    assert_eq!(config.target["x86_64-unknown-linux-gnu"].rustflags, Some(["b", "bb"].into()));

    // [target.<cfg>]
    assert_eq!(config.target["cfg(target_arch = \"x86_64\")"].linker, None);
    assert_eq!(config.target["cfg(target_arch = \"x86_64\")"].runner, Some("c".into()));
    assert_eq!(config.target["cfg(target_arch = \"x86_64\")"].rustflags, Some(["c", "cc"].into()));

    // [target.<triple>.<links>]

    // [term]
    assert_eq!(config.term.quiet, Some(false));
    assert_eq!(config.term.verbose, Some(false));
    assert_eq!(config.term.color, Some(When::Auto));
    assert_eq!(config.term.progress.when, Some(When::Auto));
    assert_eq!(config.term.progress.width, Some(80));

    // resolve
    let cx = &mut ResolveContext::no_env();
    config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    assert_eq!(config.build.target, Some("triple".into()));
    assert_eq!(config.target["x86_64-unknown-linux-gnu"].linker, Some("b".into()));
    assert_eq!(config.target["x86_64-unknown-linux-gnu"].runner, Some("b".into()));
    assert_eq!(
        config.target["x86_64-unknown-linux-gnu"].rustflags,
        Some(["b", "bb", "c", "cc"].into())
    );

    // re-resolve doesn't change the result even if env changed.
    let mut env = HashMap::<String, String>::default();
    env.insert("RUSTFLAGS".into(), "a".into());
    let cx = &mut ResolveContext::with_env(env);
    config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    assert_eq!(
        config.target["x86_64-unknown-linux-gnu"].rustflags,
        Some(["b", "bb", "c", "cc"].into())
    );

    let mut config = base_config.clone();
    let mut env = HashMap::<String, String>::default();
    env.insert("RUSTFLAGS".into(), "a".into());
    let cx = &mut ResolveContext::with_env(env);
    config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    assert_eq!(config.target["x86_64-unknown-linux-gnu"].rustflags, Some(["a"].into()));
}

#[test]
fn config_toml() {
    fn de(dir: &Path) -> Result<Config> {
        Ok(toml::from_slice(&fs::read(dir.join(".cargo/config.toml"))?)?)
    }
    fn ser(config: &Config) -> String {
        toml::to_string(&config).unwrap()
    }

    assert_reference_example(de);

    assert_eq!("", ser(&Config::default()));
}

#[test]
fn custom_target() {
    struct IsBuiltin(bool);
    #[track_caller]
    fn t(target: &str, IsBuiltin(is_builtin): IsBuiltin) {
        let spec_path = fixtures_path().join(format!("target-specs/{target}.json"));
        assert_eq!(spec_path.exists(), !is_builtin);
        let spec_path = if spec_path.exists() { spec_path.to_str().unwrap() } else { target };

        let base_config = &toml::from_str::<Config>(&format!(
            r#"
            target.{target}.linker = "avr-gcc"
            target.'cfg(target_arch = "avr")'.rustflags = "-C opt-level=s"
            "#,
        ))
        .unwrap();
        assert_eq!(
            base_config
                .build_target_for_config([spec_path], "")
                .unwrap()
                .iter()
                .map(|t| t.triple().to_owned())
                .collect::<Vec<_>>(),
            vec![target.to_owned()]
        );
        assert_eq!(base_config.build_target_for_cli([spec_path]).unwrap(), vec![
            spec_path.to_owned()
        ]);

        let mut config = base_config.clone();
        config.resolve_with_context(&mut ResolveContext::no_env(), spec_path).unwrap();
        assert_eq!(config.target[target].linker, Some("avr-gcc".into()));
        assert_eq!(config.target[target].rustflags, Some(["-C", "opt-level=s"].into()));

        let mut config = base_config.clone();
        config
            .resolve_with_context(ResolveContext::no_env().with_rustc("rustc"), spec_path)
            .unwrap();
        assert_eq!(config.target[target].linker, Some("avr-gcc".into()));
        assert_eq!(config.target[target].rustflags, Some(["-C", "opt-level=s"].into()));
    }

    t("avr-unknown-gnu-atmega328", IsBuiltin(true));
    t("avr-unknown-gnu-atmega2560", IsBuiltin(false));
}

#[test]
#[rustversion::attr(not(nightly), ignore)]
fn cargo_config_toml() {
    fn de(dir: &Path) -> Result<Config> {
        // remove CARGO_PKG_DESCRIPTION -- if field in Cargo.toml contains newline, --format=toml display invalid toml
        let s = duct::cmd!("cargo", "-Z", "unstable-options", "config", "get", "--format=toml")
            .dir(dir)
            .env_remove("CARGO_PKG_DESCRIPTION")
            .read()?;
        println!("{s}");
        toml::from_str(&s).context("failed to parse output from cargo")
    }

    //
    let _config = de(&fixtures_path().join("reference")).unwrap();
    // assert_reference_example(de);
}

#[test]
#[rustversion::attr(not(nightly), ignore)]
fn cargo_config_json() {
    fn de(dir: &Path) -> Result<Config> {
        let s = duct::cmd!("cargo", "-Z", "unstable-options", "config", "get", "--format=json",)
            .dir(dir)
            .read()?;
        serde_json::from_str(&s).context("failed to parse output from cargo")
    }

    //
    let _config = de(&fixtures_path().join("reference")).unwrap();
    // assert_reference_example(de);
}
