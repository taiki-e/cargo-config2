#![allow(clippy::bool_assert_comparison)]

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use anyhow::{Context as _, Result};
use cargo_config2::{de, easy::*};
use toml_edit::easy as toml;

fn assert_reference_example(de: fn(&Path, ResolveContext) -> Result<Config>) -> Result<()> {
    let dir = &fixtures_path().join("reference");
    let base_config = &de(dir, ResolveContext::no_env())?;
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
                assert_eq!(*v, ["run", "--release", "--", "\"command list\""].into())
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
    assert_eq!(config.env["ENV_VAR_NAME_3"].value, "relative/path");
    assert_eq!(config.env["ENV_VAR_NAME_3"].force, false);
    assert_eq!(config.env["ENV_VAR_NAME_3"].relative, true);

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
    assert_eq!(config.linker("x86_64-unknown-linux-gnu")?.unwrap().as_os_str(), "b");
    assert_eq!(config.runner("x86_64-unknown-linux-gnu")?.unwrap().path.as_os_str(), "b");
    assert!(config.runner("x86_64-unknown-linux-gnu")?.unwrap().args.is_empty());
    assert_eq!(config.rustflags("x86_64-unknown-linux-gnu")?, Some(["b", "bb", "c", "cc"].into()));

    // TODO: we should not accept cfg(.. in these functions
    // [target.<cfg>]
    assert_eq!(config.linker("cfg(target_arch = \"x86_64\")")?, None);
    assert_eq!(config.runner("cfg(target_arch = \"x86_64\")")?.unwrap().path.as_os_str(), "c");
    assert!(config.runner("cfg(target_arch = \"x86_64\")")?.unwrap().args.is_empty());
    assert_eq!(config.rustflags("cfg(target_arch = \"x86_64\")")?, Some(["c", "cc"].into()));

    // [target.<triple>.<links>]

    // [term]
    assert_eq!(config.term.quiet, Some(false));
    assert_eq!(config.term.verbose, Some(false));
    assert_eq!(config.term.color, Some(When::Auto));
    assert_eq!(config.term.progress.when, Some(When::Auto));
    assert_eq!(config.term.progress.width, Some(80));

    // // resolve
    // let mut config = base_config.clone();
    // let cx = &mut ResolveContext::no_env();
    // config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    // assert_eq!(config.build.target.as_ref().unwrap().string().unwrap().val, "triple");
    // assert_eq!(config.target["x86_64-unknown-linux-gnu"].linker.as_ref().unwrap().val, "b");
    // assert_eq!(
    //     config.target["x86_64-unknown-linux-gnu"].runner.as_ref().unwrap().string().unwrap().val,
    //     "b"
    // );
    // assert_eq!(
    //     config.target["x86_64-unknown-linux-gnu"].rustflags,
    //     Some(["b", "bb", "c", "cc"].into())
    // );

    // // re-resolve doesn't change the result even if env changed.
    // let mut env = HashMap::<String, String>::default();
    // env.insert("RUSTFLAGS".into(), "a".into());
    // let cx = &mut ResolveContext::from_env(env);
    // config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    // assert_eq!(
    //     config.target["x86_64-unknown-linux-gnu"].rustflags,
    //     Some(["b", "bb", "c", "cc"].into())
    // );

    // let mut config = base_config.clone();
    // let mut env = HashMap::<String, String>::default();
    // env.insert("RUSTFLAGS".into(), "a".into());
    // let cx = &mut ResolveContext::from_env(env);
    // config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    // assert_eq!(config.target["x86_64-unknown-linux-gnu"].rustflags, Some(["a"].into()));

    Ok(())
}

#[test]
fn easy() {
    fn de(dir: &Path, cx: ResolveContext) -> Result<Config> {
        Config::load_with_context(dir, None, cx)
    }
    #[track_caller]
    fn ser(config: &Config) -> String {
        toml::to_string(&config).unwrap()
    }

    assert_reference_example(de).unwrap();

    assert_eq!("", toml::to_string(&de::Config::default()).unwrap());
}

#[test]
fn de() {
    fn de(dir: &Path, _cx: ResolveContext) -> Result<de::Config> {
        de::Config::load_with_context(dir, None)
    }
    #[track_caller]
    fn ser(config: &de::Config) -> String {
        toml::to_string(&config).unwrap()
    }

    // assert_reference_example(de).unwrap();

    assert_eq!("", ser(&de::Config::default()));
}

#[test]
fn lazy() -> Result<()> {
    use cargo_config2::lazy::*;

    let (_tmp, root) = test_project("reference")?;
    let dir = &root;
    let config = Config::with_context(dir, None, ResolveContext::no_env());

    // [alias]
    // for (k, v) in &config.alias {
    //     match k.as_str() {
    //         "b" => assert_eq!(*v, "build".into()),
    //         "c" => assert_eq!(*v, "check".into()),
    //         "t" => assert_eq!(*v, "test".into()),
    //         "r" => assert_eq!(*v, "run".into()),
    //         "rr" => assert_eq!(*v, "run --release".into()),
    //         "recursive_example" => assert_eq!(*v, "rr --example recursions".into()),
    //         "space_example" => {
    //             assert_eq!(*v, ["run", "--release", "--", "\"command list\""].into())
    //         }
    //         _ => panic!("unexpected alias: name={k}, value={v:?}"),
    //     }
    // }

    // [build]
    let build = config.build_config()?;
    assert_eq!(build.rustc.as_ref().unwrap().as_os_str(), "rustc");
    assert_eq!(build.rustc_wrapper.as_ref().unwrap().as_os_str(), "…");
    assert_eq!(build.rustc_workspace_wrapper.as_ref().unwrap().as_os_str(), "…");
    assert_eq!(build.rustdoc.as_ref().unwrap().as_os_str(), "rustdoc");
    assert_eq!(build.target.as_ref().unwrap(), &vec!["triple".into()]);
    assert_eq!(build.target_dir.as_ref().unwrap(), &dir.join("target"));
    // TODO
    // assert_eq!(build.rustflags, Some(["…", "…"].into()));
    assert_eq!(build.rustdocflags, Some(["…", "…"].into()));
    assert_eq!(build.incremental, Some(true));
    assert_eq!(build.dep_info_basedir.as_ref().unwrap(), &dir.join("…"));

    // [doc]
    let doc = config.doc_config()?;
    assert_eq!(doc.browser.as_ref().unwrap().path.as_os_str(), "chromium");
    assert!(doc.browser.as_ref().unwrap().args.is_empty());

    // [env]
    // assert_eq!(env["ENV_VAR_NAME"].value, "value");
    // assert_eq!(env["ENV_VAR_NAME"].force, false);
    // assert_eq!(env["ENV_VAR_NAME"].relative, false);
    // assert_eq!(env["ENV_VAR_NAME_2"].value, "value");
    // assert_eq!(env["ENV_VAR_NAME_2"].force, true);
    // assert_eq!(env["ENV_VAR_NAME_2"].relative, false);
    // assert_eq!(env["ENV_VAR_NAME_3"].value, "relative/path");
    // assert_eq!(env["ENV_VAR_NAME_3"].force, false);
    // assert_eq!(env["ENV_VAR_NAME_3"].relative, true);

    // [future-incompat-report]
    let future_incompat_report = config.future_incompat_report_config()?;
    assert_eq!(future_incompat_report.frequency, Some(Frequency::Always));

    // TODO
    // [cargo-new]

    // TODO
    // [http]

    // TODO
    // [install]

    // [net]
    let net = config.net_config()?;
    assert_eq!(net.retry, Some(2));
    assert_eq!(net.git_fetch_with_cli, Some(true));
    assert_eq!(net.offline, Some(true));

    // TODO
    // [patch.<registry>]
    // [profile.<name>]
    // [registries.<name>]
    // [registry]
    // [source.<name>]

    // // [target.<triple>]
    // assert_eq!(config.linker("x86_64-unknown-linux-gnu")?.unwrap().as_os_str(), "b");
    // assert_eq!(config.runner("x86_64-unknown-linux-gnu")?.unwrap().path.as_os_str(), "b");
    // assert!(config.runner("x86_64-unknown-linux-gnu")?.unwrap().args.is_empty());
    // assert_eq!(config.rustflags("x86_64-unknown-linux-gnu")?, Some(["b", "bb", "c", "cc"].into()));

    // // TODO: we should not accept cfg(.. in these functions
    // // [target.<cfg>]
    // assert_eq!(config.linker("cfg(target_arch = \"x86_64\")")?, None);
    // assert_eq!(config.runner("cfg(target_arch = \"x86_64\")")?.unwrap().path.as_os_str(), "c");
    // assert!(config.runner("cfg(target_arch = \"x86_64\")")?.unwrap().args.is_empty());
    // assert_eq!(config.rustflags("cfg(target_arch = \"x86_64\")")?, Some(["c", "cc"].into()));

    // [target.<triple>.<links>]

    // [term]
    let term = config.term_config()?;
    assert_eq!(term.quiet, Some(false));
    assert_eq!(term.verbose, Some(false));
    assert_eq!(term.color, Some(When::Auto));
    assert_eq!(term.progress.when, Some(When::Auto));
    assert_eq!(term.progress.width, Some(80));

    // // resolve
    // let mut config = base_config.clone();
    // let cx = &mut ResolveContext::no_env();
    // config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    // assert_eq!(config.build.target.as_ref().unwrap().string().unwrap().val, "triple");
    // assert_eq!(config.target["x86_64-unknown-linux-gnu"].linker.as_ref().unwrap().val, "b");
    // assert_eq!(
    //     config.target["x86_64-unknown-linux-gnu"].runner.as_ref().unwrap().string().unwrap().val,
    //     "b"
    // );
    // assert_eq!(
    //     config.target["x86_64-unknown-linux-gnu"].rustflags,
    //     Some(["b", "bb", "c", "cc"].into())
    // );

    // // re-resolve doesn't change the result even if env changed.
    // let mut env = HashMap::<String, String>::default();
    // env.insert("RUSTFLAGS".into(), "a".into());
    // let cx = &mut ResolveContext::from_env(env);
    // config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    // assert_eq!(
    //     config.target["x86_64-unknown-linux-gnu"].rustflags,
    //     Some(["b", "bb", "c", "cc"].into())
    // );

    // let mut config = base_config.clone();
    // let mut env = HashMap::<String, String>::default();
    // env.insert("RUSTFLAGS".into(), "a".into());
    // let cx = &mut ResolveContext::from_env(env);
    // config.resolve_with_context(cx, "x86_64-unknown-linux-gnu").unwrap();
    // assert_eq!(config.target["x86_64-unknown-linux-gnu"].rustflags, Some(["a"].into()));

    Ok(())
}

// #[test]
// fn custom_target() {
//     struct IsBuiltin(bool);
//     #[track_caller]
//     fn t(target: &str, IsBuiltin(is_builtin): IsBuiltin) {
//         let spec_path = fixtures_path().join(format!("target-specs/{target}.json"));
//         assert_eq!(spec_path.exists(), !is_builtin);
//         let spec_path = if spec_path.exists() { spec_path.to_str().unwrap() } else { target };

//         let base_config = &toml::from_str::<de::Config>(&format!(
//             r#"
//             target.{target}.linker = "avr-gcc"
//             target.'cfg(target_arch = "avr")'.rustflags = "-C opt-level=s"
//             "#,
//         ))
//         .unwrap();
//         assert_eq!(
//             base_config
//                 .build_target_for_config([spec_path], "")
//                 .unwrap()
//                 .iter()
//                 .map(|t| t.triple().to_owned())
//                 .collect::<Vec<_>>(),
//             vec![target.to_owned()]
//         );
//         assert_eq!(base_config.build_target_for_cli([spec_path]).unwrap(), vec![
//             spec_path.to_owned()
//         ]);

//         let mut config = base_config.clone();
//         config.resolve_with_context(&mut ResolveContext::no_env(), spec_path).unwrap();
//         assert_eq!(config.target[target].linker.as_ref().unwrap().val, "avr-gcc");
//         assert_eq!(config.target[target].rustflags, Some(["-C", "opt-level=s"].into()));

//         let mut config = base_config.clone();
//         config
//             .resolve_with_context(ResolveContext::no_env().set_rustc("rustc"), spec_path)
//             .unwrap();
//         assert_eq!(config.target[target].linker.as_ref().unwrap().val, "avr-gcc");
//         assert_eq!(config.target[target].rustflags, Some(["-C", "opt-level=s"].into()));
//     }

//     t("avr-unknown-gnu-atmega328", IsBuiltin(true));
//     t("avr-unknown-gnu-atmega2560", IsBuiltin(false));
// }

// #[test]
// #[rustversion::attr(not(nightly), ignore)]
// fn cargo_config_toml() {
//     fn de(dir: &Path) -> Result<Config> {
//         // remove CARGO_PKG_DESCRIPTION -- if field in Cargo.toml contains newline, --format=toml display invalid toml
//         let s = duct::cmd!("cargo", "-Z", "unstable-options", "config", "get", "--format=toml")
//             .dir(dir)
//             .env_remove("CARGO_PKG_DESCRIPTION")
//             .read()?;
//         println!("{s}");
//         toml::from_str(&s).context("failed to parse output from cargo")
//     }

//     //
//     let _config = de(&fixtures_path().join("reference")).unwrap();
//     // assert_reference_example(de);
// }

// #[test]
// #[rustversion::attr(not(nightly), ignore)]
// fn cargo_config_json() {
//     fn de(dir: &Path) -> Result<Config> {
//         let s = duct::cmd!("cargo", "-Z", "unstable-options", "config", "get", "--format=json",)
//             .dir(dir)
//             .read()?;
//         serde_json::from_str(&s).context("failed to parse output from cargo")
//     }

//     //
//     let _config = de(&fixtures_path().join("reference")).unwrap();
//     // assert_reference_example(de);
// }

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
