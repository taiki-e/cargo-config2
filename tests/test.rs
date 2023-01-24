#![allow(clippy::bool_assert_comparison, clippy::needless_pass_by_value)]

use std::{collections::HashMap, path::Path, str};

use anyhow::{Context as _, Result};
use build_info::TARGET;
use cargo_config2::*;

fn test_options() -> ResolveOptions {
    ResolveOptions::default()
        .env(HashMap::<String, String>::default())
        .cargo_home(None)
        .rustc(PathAndArgs::new("rustc"))
}

fn assert_reference_example(de: fn(&Path, ResolveOptions) -> Result<Config>) -> Result<()> {
    let (_tmp, root) = test_project("reference")?;
    let dir = &root;
    let base_config = &de(dir, test_options())?;
    let config = base_config.clone();

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
                assert_eq!(*v, ["run", "--release", "--", "\"command list\""].into());
            }
            _ => panic!("unexpected alias: name={k}, value={v:?}"),
        }
    }

    // [build]
    assert_eq!(config.build.rustc.as_ref().unwrap().as_os_str(), "rustc");
    assert_eq!(config.build.rustc_wrapper.as_ref().unwrap().as_os_str(), "…");
    assert_eq!(config.build.rustc_workspace_wrapper.as_ref().unwrap().as_os_str(), "…");
    assert_eq!(config.build.rustdoc.as_ref().unwrap().as_os_str(), "rustdoc");
    assert_eq!(config.build.target.as_ref().unwrap(), &vec!["triple".into()]);
    assert_eq!(config.build.target_dir.as_ref().unwrap(), &dir.join("target"));
    // TODO
    // assert_eq!(config.build.rustflags, Some(["…", "…"].into()));
    assert_eq!(config.build.rustdocflags, Some(["…", "…"].into()));
    assert_eq!(config.build.incremental, Some(true));
    assert_eq!(config.build.dep_info_basedir.as_ref().unwrap(), &dir.join("…"));

    // [doc]
    assert_eq!(config.doc.browser.as_ref().unwrap().path.as_os_str(), "chromium");
    assert!(config.doc.browser.as_ref().unwrap().args.is_empty());

    // [env]
    assert_eq!(config.env["ENV_VAR_NAME"].value, "value");
    assert_eq!(config.env["ENV_VAR_NAME"].force, false);
    assert_eq!(config.env["ENV_VAR_NAME"].relative, false);
    assert_eq!(config.env["ENV_VAR_NAME_2"].value, "value");
    assert_eq!(config.env["ENV_VAR_NAME_2"].force, true);
    assert_eq!(config.env["ENV_VAR_NAME_2"].relative, false);
    assert_eq!(config.env["ENV_VAR_NAME_3"].value, dir.join("relative/path"));
    assert_eq!(config.env["ENV_VAR_NAME_3"].force, false);
    assert_eq!(config.env["ENV_VAR_NAME_3"].relative, false); // false because it has been resolved

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

    // [target.<triple>] and [target.<cfg>]
    assert_eq!(config.target("x86_64-unknown-linux-gnu")?.linker.unwrap().as_os_str(), "b");
    assert_eq!(config.target("x86_64-unknown-linux-gnu")?.runner.unwrap().path.as_os_str(), "b");
    assert!(config.target("x86_64-unknown-linux-gnu")?.runner.unwrap().args.is_empty());
    assert_eq!(
        config.target("x86_64-unknown-linux-gnu")?.rustflags,
        Some(["b", "bb", "c", "cc"].into())
    );
    assert_eq!(config.linker("x86_64-unknown-linux-gnu")?.unwrap().as_os_str(), "b");
    assert_eq!(config.runner("x86_64-unknown-linux-gnu")?.unwrap().path.as_os_str(), "b");
    assert!(config.runner("x86_64-unknown-linux-gnu")?.unwrap().args.is_empty());
    assert_eq!(config.rustflags("x86_64-unknown-linux-gnu")?, Some(["b", "bb", "c", "cc"].into()));
    // TODO: [target.<triple>.<links>]

    // resolved target config cannot be accessed by cfg(...)
    assert!(config
        .target("cfg(target_arch = \"x86_64\")")
        .unwrap_err()
        .to_string()
        .contains("not valid target triple"));
    assert!(config
        .linker("cfg(target_arch = \"x86_64\")")
        .unwrap_err()
        .to_string()
        .contains("not valid target triple"));
    assert!(config
        .runner("cfg(target_arch = \"x86_64\")")
        .unwrap_err()
        .to_string()
        .contains("not valid target triple"));
    assert!(config
        .rustflags("cfg(target_arch = \"x86_64\")")
        .unwrap_err()
        .to_string()
        .contains("not valid target triple"));

    // [term]
    assert_eq!(config.term.quiet, Some(false));
    assert_eq!(config.term.verbose, Some(false));
    assert_eq!(config.term.color, Some(Color::Auto));
    assert_eq!(config.term.progress.when, Some(When::Auto));
    assert_eq!(config.term.progress.width, Some(80));

    let _config = toml::to_string(&config).unwrap();

    Ok(())
}

fn easy_load(dir: &Path, options: ResolveOptions) -> Result<Config> {
    Ok(Config::load_with_options(dir, options)?)
}
#[test]
fn easy() {
    use easy_load as de;

    assert_reference_example(de).unwrap();
}

fn de_load(dir: &Path, _cx: ResolveOptions) -> Result<de::Config> {
    Ok(de::Config::load_with_options(dir, None)?)
}
#[test]
fn de() {
    use de_load as de;
    #[track_caller]
    fn ser(config: &de::Config) -> String {
        toml::to_string(&config).unwrap()
    }

    let (_tmp, root) = test_project("reference").unwrap();
    let dir = &root;
    let base_config = &de(dir, test_options()).unwrap();
    let config = base_config.clone();

    let _config = toml::to_string(&config).unwrap();

    assert_eq!("", ser(&de::Config::default()));
}

#[test]
fn custom_target() {
    use easy_load as de;
    struct IsBuiltin(bool);
    #[track_caller]
    fn t(target: &str, IsBuiltin(is_builtin): IsBuiltin) -> Result<()> {
        let (_tmp, root) = test_project("empty")?;
        let dir = &root;
        fs::write(
            root.join(".cargo/config.toml"),
            format!(
                r#"
                target.{target}.linker = "avr-gcc"
                target.'cfg(target_arch = "avr")'.rustflags = "-C opt-level=s"
                "#
            ),
        )?;
        let spec_path = fixtures_path().join(format!("target-specs/{target}.json"));
        assert_eq!(spec_path.exists(), !is_builtin);
        let cli_target = if spec_path.exists() { spec_path.to_str().unwrap() } else { target };

        let config = de(dir, test_options())?;
        assert_eq!(
            config
                .build_target_for_config([cli_target])?
                .iter()
                .map(|t| t.triple().to_owned())
                .collect::<Vec<_>>(),
            vec![target.to_owned()]
        );
        assert_eq!(config.build_target_for_cli([cli_target])?, vec![cli_target.to_owned()]);

        assert_eq!(config.linker(cli_target)?.unwrap().as_os_str(), "avr-gcc");
        assert_eq!(config.rustflags(cli_target)?, Some(["-C", "opt-level=s"].into()));

        // only resolve relative path from config or environment variables
        let spec_file_name = spec_path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            config.build_target_for_config([spec_file_name])?[0].spec_path().unwrap().as_os_str(),
            spec_file_name
        );
        assert_eq!(config.build_target_for_cli([spec_file_name])?, vec![spec_file_name.to_owned()]);

        let _config = toml::to_string(&config).unwrap();

        Ok(())
    }

    t("avr-unknown-gnu-atmega328", IsBuiltin(true)).unwrap();
    t("avr-unknown-gnu-atmega2560", IsBuiltin(false)).unwrap();
}

#[rustversion::attr(not(nightly), ignore)]
#[test]
fn cargo_config_toml() {
    fn de(dir: &Path) -> Result<de::Config> {
        // remove CARGO_PKG_DESCRIPTION -- if field in Cargo.toml contains newline, --format=toml display invalid toml
        let s = duct::cmd!("cargo", "-Z", "unstable-options", "config", "get", "--format=toml")
            .dir(dir)
            .env_remove("CARGO_PKG_DESCRIPTION")
            .stderr_capture()
            .read()?;
        toml::from_str(&s).context("failed to parse output from cargo")
    }

    let _config = de(&fixtures_path().join("reference")).unwrap();
}

#[rustversion::attr(not(nightly), ignore)]
#[test]
fn cargo_config_json() {
    fn de(dir: &Path) -> Result<de::Config> {
        let s = duct::cmd!("cargo", "-Z", "unstable-options", "config", "get", "--format=json",)
            .dir(dir)
            .stderr_capture()
            .read()?;
        serde_json::from_str(&s).context("failed to parse output from cargo")
    }

    let _config = de(&fixtures_path().join("reference")).unwrap();
}

#[test]
fn test_cargo_behavior() -> Result<()> {
    let (_tmp, root) = test_project("empty").unwrap();
    let dir = &root;

    // [env] table doesn't affect config resolution
    // https://github.com/taiki-e/cargo-config2/issues/2
    fs::write(
        root.join(".cargo/config.toml"),
        r#"
            [env]
            RUSTFLAGS = "--cfg a"
            [build]
            rustflags = "--cfg b"
            "#,
    )?;
    let output = duct::cmd!("cargo", "build", "-v")
        .dir(dir)
        .env("CARGO_HOME", root.join(".cargo"))
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove(format!("CARGO_TARGET_{}_RUSTFLAGS", TARGET.replace(['-', '.'], "_")))
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .stdout_capture()
        .stderr_capture()
        .run()?;
    let stderr = str::from_utf8(&output.stderr)?;
    assert!(!stderr.contains("--cfg a"), "actual:\n---\n{stderr}\n---\n");
    assert!(stderr.contains("--cfg b"), "actual:\n---\n{stderr}\n---\n");

    Ok(())
}

use helper::*;
mod helper {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    pub use fs_err as fs;

    pub fn fixtures_path() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
    }

    pub fn test_project(model: &str) -> Result<(tempfile::TempDir, PathBuf)> {
        let tmpdir = tempfile::tempdir()?;
        let tmpdir_path = tmpdir.path();

        let model_path;
        let workspace_root;
        if model.contains('/') {
            let mut model = model.splitn(2, '/');
            model_path = fixtures_path().join(model.next().unwrap());
            workspace_root = tmpdir_path.join(model.next().unwrap());
            assert!(model.next().is_none());
        } else {
            model_path = fixtures_path().join(model);
            workspace_root = tmpdir_path.to_path_buf();
        }

        for entry in walkdir::WalkDir::new(&model_path).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            let tmppath = &tmpdir_path.join(path.strip_prefix(&model_path)?);
            if !tmppath.exists() {
                if path.is_dir() {
                    fs::create_dir_all(tmppath)?;
                } else {
                    fs::copy(path, tmppath)?;
                }
            }
        }

        Ok((tmpdir, workspace_root))
    }
}
