use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    num::NonZeroI32,
    path::{Path, PathBuf},
};

use anyhow::Result;
use once_cell::unsync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::de;
#[doc(no_inline)]
pub use crate::de::{Color, Frequency, When};
pub use crate::{
    command::host_triple,
    easy_old::{
        DocConfig, EnvConfigValue, FutureIncompatReportConfig, NetConfig, PathAndArgs, Rustflags,
        StringList, TermConfig,
    },
    paths::ConfigPaths,
    resolve::{ResolveContext, TargetTriple, TargetTripleRef},
    value::{Definition, Value},
};

#[derive(Debug)]
pub struct Config {
    de: UnresolvedConfig,
    // TODO: paths
    /// Command aliases.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#alias)
    alias: OnceCell<BTreeMap<String, StringList>>,
    build: OnceCell<BuildConfig>,
    doc: OnceCell<DocConfig>,
    env: OnceCell<BTreeMap<String, EnvConfigValue>>,
    future_incompat_report: OnceCell<FutureIncompatReportConfig>,
    // TODO: cargo-new
    // TODO: http
    // TODO: install
    net: OnceCell<NetConfig>,
    // TODO: patch
    // TODO: profile
    // TODO: registries
    // TODO: registry
    // TODO: source
    target: RefCell<BTreeMap<TargetTriple, TargetConfig>>,
    term: OnceCell<TermConfig>,

    // Resolve contexts.
    resolve_context: ResolveContext,
    current_dir: PathBuf,
}

struct UnresolvedConfig {
    alias: Cell<BTreeMap<String, de::StringList>>,
    build: Cell<de::BuildConfig>,
    doc: Cell<de::DocConfig>,
    env: Cell<BTreeMap<String, de::EnvConfigValue>>,
    future_incompat_report: Cell<de::FutureIncompatReportConfig>,
    // TODO: cargo-new
    // TODO: http
    // TODO: install
    net: Cell<de::NetConfig>,
    // TODO: patch
    // TODO: profile
    // TODO: registries
    // TODO: registry
    // TODO: source
    target: BTreeMap<String, de::TargetConfig>,
    term: Cell<de::TermConfig>,

    build_rustflags: Option<de::Rustflags>,
    override_target_rustflags: bool,
}

impl From<de::Config> for UnresolvedConfig {
    fn from(value: de::Config) -> Self {
        let de::Config {
            alias,
            build,
            doc,
            env,
            future_incompat_report,
            net,
            target,
            term,
            current_dir: _,
            path: _,
        } = value;
        Self {
            build_rustflags: build.rustflags.clone(),
            override_target_rustflags: build.override_target_rustflags,
            alias: Cell::new(alias),
            build: Cell::new(build),
            doc: Cell::new(doc),
            env: Cell::new(env),
            future_incompat_report: Cell::new(future_incompat_report),
            net: Cell::new(net),
            target,
            term: Cell::new(term),
        }
    }
}

impl fmt::Debug for UnresolvedConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UnresolvedConfig { .. }")
    }
}

impl Config {
    /// Read config files hierarchically from the current directory and merges them.
    #[cfg(feature = "toml")]
    #[cfg_attr(docsrs, doc(cfg(feature = "toml")))]
    pub fn load() -> Result<Self> {
        let mut resolve_context = ResolveContext::new()?;
        let current_dir = std::env::current_dir()?;
        let mut de = de::toml::read_hierarchical(&current_dir)?.unwrap_or_default();
        de.apply_env(&mut resolve_context)?;
        Ok(Self {
            de: UnresolvedConfig::from(de),
            alias: OnceCell::new(),
            build: OnceCell::new(),
            doc: OnceCell::new(),
            env: OnceCell::new(),
            future_incompat_report: OnceCell::new(),
            net: OnceCell::new(),
            target: RefCell::new(BTreeMap::default()),
            term: OnceCell::new(),
            resolve_context,
            current_dir,
        })
    }

    /// Gets the `[alias]` table.
    pub fn alias(&self) -> &BTreeMap<String, StringList> {
        self.alias.get_or_init(|| {
            let mut map: BTreeMap<String, StringList> = BTreeMap::default();
            for (k, v) in self.de.alias.take() {
                map.insert(k, v.into());
            }
            map
        })
    }
    /// Gets the `[build]` table.
    pub fn build(&self) -> Result<&BuildConfig> {
        self.build.get_or_try_init(|| {
            BuildConfig::from_unresolved(self.de.build.take(), Some(&self.current_dir))
        })
    }
    /// Gets the `[doc]` table.
    pub fn doc(&self) -> Result<&DocConfig> {
        self.doc.get_or_try_init(|| {
            DocConfig::from_unresolved(self.de.doc.take(), Some(&self.current_dir))
        })
    }
    /// Gets the `[env]` table.
    pub fn env(&self) -> &BTreeMap<String, EnvConfigValue> {
        self.env.get_or_init(|| {
            let mut map: BTreeMap<String, EnvConfigValue> = BTreeMap::default();
            for (k, v) in self.de.env.take() {
                map.insert(k, EnvConfigValue::from_unresolved(v));
            }
            map
        })
    }
    /// Gets the `[future_incompat_report]` table.
    pub fn future_incompat_report(&self) -> Result<&FutureIncompatReportConfig> {
        self.future_incompat_report.get_or_try_init(|| {
            FutureIncompatReportConfig::from_unresolved(self.de.future_incompat_report.take())
        })
    }
    /// Gets the `[net]` table.
    pub fn net(&self) -> Result<&NetConfig> {
        self.net.get_or_try_init(|| NetConfig::from_unresolved(self.de.net.take()))
    }
    /// Gets the `[target]` table.
    pub fn target(&self, target: &TargetTripleRef<'_>) -> Result<TargetConfig> {
        let mut target_configs = self.target.borrow_mut();
        if !target_configs.contains_key(target) {
            let target_config = TargetConfig::from_unresolved(
                de::Config::resolve_target(
                    &self.resolve_context,
                    &self.de.target,
                    self.de.override_target_rustflags,
                    &self.de.build_rustflags,
                    target,
                )?
                .unwrap_or_default(),
                Some(&self.current_dir),
            )?;
            target_configs.insert(target.clone().into_owned(), target_config);
        }
        Ok(target_configs[target].clone())
    }
    /// Gets the `[term]` table.
    pub fn term(&self) -> Result<&TermConfig> {
        self.term.get_or_try_init(|| Ok(TermConfig::from_unresolved(self.de.term.take())))
    }

    // pub(crate) fn from_unresolved(
    //     mut de: de::Config,
    //     targets: &[TargetTriple],
    //     mut cx: ResolveContext,
    // ) -> Result<Self> {
    //     let mut resolved = Self::default();
    //     de.apply_env(&mut cx)?;

    //     for (k, v) in de.alias {
    //         resolved.alias.insert(k, v.into());
    //     }
    //     resolved.build =
    //         BuildConfig::from_unresolved(de.build, de.current_dir.as_deref(), &mut cx)?;
    //     resolved.doc.from_unresolved(de.doc, de.current_dir.as_deref(), &mut cx)?;
    //     for (k, v) in de.env {
    //         // TODO
    //     }
    //     resolved.future_incompat_report.from_unresolved(
    //         de.future_incompat_report,
    //         de.current_dir.as_deref(),
    //         &mut cx,
    //     )?;
    //     resolved.net.from_unresolved(de.net, de.current_dir.as_deref(), &mut cx)?;
    //     resolved.term.from_unresolved(de.term, de.current_dir.as_deref(), &mut cx)?;
    //     for (k, v) in de.target {
    //         // TODO
    //     }

    //     // TODO: target

    //     resolved.resolve_context = Some(cx);
    //     Ok(resolved)
    // }

    /// Selects target triples to build.
    ///
    /// The targets returned are based on the order of priority in which cargo
    /// selects the target to be used for the build.
    ///
    /// 1. `--target` option (`targets`)
    /// 2. `CARGO_BUILD_TARGET` environment variable
    /// 3. `build.target` config
    /// 4. host triple (`host`)
    ///
    /// **Note:** The result of this function is intended to handle target-specific
    /// configurations and is not always appropriate to propagate directly to Cargo.
    /// See [`build_target_for_cli`](Self::build_target_for_cli) for more.
    ///
    /// ## Multi-target support
    ///
    /// [Cargo 1.64+ supports multi-target builds](https://blog.rust-lang.org/2022/09/22/Rust-1.64.0.html#cargo-improvements-workspace-inheritance-and-multi-target-builds).
    ///
    /// Therefore, this function may return multiple targets if multiple targets
    /// are specified in `targets` or `build.target` config.
    ///
    /// ## Custom target support
    ///
    /// rustc allows you to build a custom target by specifying a target-spec file.
    /// If a target-spec file is specified as the target, rustc considers the
    /// [file stem](Path::file_stem) of that file to be the target triple name.
    ///
    /// Since target-specific configs are referred by target triple name, this
    /// function also converts the target specified in the path to a target triple name.
    ///
    /// ## Examples
    ///
    /// With single-target:
    ///
    /// ```no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use anyhow::bail;
    /// use clap::Parser;
    ///
    /// #[derive(Parser)]
    /// struct Args {
    ///     #[clap(long)]
    ///     target: Option<String>,
    /// }
    ///
    /// let args = Args::parse();
    /// let mut config = cargo_config2::Config::load()?;
    ///
    /// let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    /// let host = cargo_config2::host_triple(cargo)?;
    /// let mut targets = config.build_target_for_config(args.target.as_ref(), &host)?;
    /// if targets.len() != 1 {
    ///     bail!("multi-target build is not supported: {targets:?}");
    /// }
    /// let target = targets.pop().unwrap();
    ///
    /// config.resolve(&target)?;
    /// println!("{:?}", config[target].rustflags);
    /// # Ok(()) }
    /// ```
    ///
    /// With multi-target:
    ///
    /// ```no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use clap::Parser;
    ///
    /// #[derive(Parser)]
    /// struct Args {
    ///     #[clap(long)]
    ///     target: Vec<String>,
    /// }
    ///
    /// let args = Args::parse();
    /// let mut config = cargo_config2::Config::load()?;
    ///
    /// let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    /// let host = cargo_config2::host_triple(cargo)?;
    /// let targets = config.build_target_for_config(&args.target, &host)?;
    ///
    /// for target in targets {
    ///     config.resolve(&target)?;
    ///     println!("{:?}", config[target].rustflags);
    /// }
    /// # Ok(()) }
    /// ```
    #[allow(single_use_lifetimes)] // https://github.com/rust-lang/rust/issues/105705
    pub fn build_target_for_config<'a, 'b>(
        &self,
        targets: impl IntoIterator<Item = impl Into<TargetTripleRef<'a>>>,
        host: impl Into<TargetTripleRef<'b>>,
    ) -> Result<Vec<TargetTriple>> {
        let targets: Vec<_> = targets.into_iter().map(|v| v.into().into_owned()).collect();
        if !targets.is_empty() {
            return Ok(targets);
        }
        let config_targets = self.build()?.target.clone().unwrap_or_default();
        if !config_targets.is_empty() {
            return Ok(config_targets);
        }
        Ok(vec![host.into().into_owned()])
    }

    /// Selects target triples to pass to CLI.
    ///
    /// The targets returned are based on the order of priority in which cargo
    /// selects the target to be used for the build.
    ///
    /// 1. `--target` option (`targets`)
    /// 2. `CARGO_BUILD_TARGET` environment variable
    /// 3. `build.target` config
    ///
    /// Unlike [`build_target_for_config`](Self::build_target_for_config),
    /// host triple is not referenced. This is because the behavior of Cargo
    /// changes depending on whether or not `--target` option (or one of the
    /// above) is set.
    /// Also, Unlike [`build_target_for_config`](Self::build_target_for_config)
    /// the target name specified in path is preserved.
    pub fn build_target_for_cli(
        &self,
        targets: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Vec<String>> {
        let targets: Vec<_> = targets.into_iter().map(|t| t.as_ref().to_owned()).collect();
        if !targets.is_empty() {
            return Ok(targets);
        }
        let config_targets = self.build()?.target.as_deref().unwrap_or_default();
        if !config_targets.is_empty() {
            return Ok(config_targets
                .iter()
                .map(|t| t.spec_path().unwrap_or(t.triple()).to_owned())
                .collect());
        }
        Ok(vec![])
    }

    // TODO: add override instead?
    // /// Merges the given config into this config.
    // ///
    // /// If `force` is `false`, this matches the way cargo [merges configs in the
    // /// parent directories](https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure).
    // ///
    // /// If `force` is `true`, this matches the way cargo's `--config` CLI option
    // /// overrides config.
    // pub fn merge(&mut self, from: Self, force: bool) -> Result<()> {
    //     merge::Merge::merge(self, from, force)
    // }
}

// impl<T: Borrow<TargetTriple>> ops::Index<T> for Config {
//     type Output = TargetConfig;
//     fn index(&self, index: T) -> &Self::Output {
//         &self.target[&index.borrow().triple]
//     }
// }

/// The `[build]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#build)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct BuildConfig {
    /// Sets the maximum number of compiler processes to run in parallel.
    /// If negative, it sets the maximum number of compiler processes to the
    /// number of logical CPUs plus provided value. Should not be 0.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildjobs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jobs: Option<NonZeroI32>,
    /// Sets the executable to use for `rustc`.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustc: Option<PathBuf>,
    /// Sets a wrapper to execute instead of `rustc`.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-wrapper)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustc_wrapper: Option<PathBuf>,
    /// Sets a wrapper to execute instead of `rustc`, for workspace members only.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-workspace-wrapper)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustc_workspace_wrapper: Option<PathBuf>,
    /// Sets the executable to use for `rustdoc`.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdoc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustdoc: Option<PathBuf>,
    /// The default target platform triples to compile to.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Vec<TargetTriple>>,
    /// The path to where all compiler output is placed. The default if not
    /// specified is a directory named target located at the root of the workspace.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_dir: Option<PathBuf>,
    /// Extra command-line flags to pass to rustc. The value may be an array
    /// of strings or a space-separated string.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rustflags: Option<Rustflags>,
    /// Extra command-line flags to pass to `rustdoc`. The value may be an array
    /// of strings or a space-separated string.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustdocflags: Option<Rustflags>,
    /// Whether or not to perform incremental compilation.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildincremental)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental: Option<bool>,
    /// Strips the given path prefix from dep info file paths.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#builddep-info-basedir)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dep_info_basedir: Option<PathBuf>,
}

impl BuildConfig {
    pub(crate) fn from_unresolved(de: de::BuildConfig, current_dir: Option<&Path>) -> Result<Self> {
        let jobs = de.jobs.map(|v| v.val);
        let rustc = de.rustc.map(|v| v.resolve_as_program_path(current_dir).into_owned());
        let rustc_wrapper =
            de.rustc_wrapper.map(|v| v.resolve_as_program_path(current_dir).into_owned());
        let rustc_workspace_wrapper =
            de.rustc_workspace_wrapper.map(|v| v.resolve_as_program_path(current_dir).into_owned());
        let rustdoc = de.rustdoc.map(|v| v.resolve_as_program_path(current_dir).into_owned());
        let target = de.target.map(|t| {
            t.as_array_no_split()
                .iter()
                .map(|v| {
                    TargetTriple::new(v.val.clone().into(), v.definition.as_ref(), current_dir)
                })
                .collect()
        });
        let target_dir = de.target_dir.map(|v| v.resolve_as_path(current_dir).into_owned());
        let rustflags =
            de.rustflags.map(|v| Rustflags { flags: v.flags.into_iter().map(|v| v.val).collect() });
        let rustdocflags = de
            .rustdocflags
            .map(|v| Rustflags { flags: v.flags.into_iter().map(|v| v.val).collect() });
        let incremental = de.incremental.map(|v| v.val);
        let dep_info_basedir =
            de.dep_info_basedir.map(|v| v.resolve_as_path(current_dir).into_owned());
        Ok(Self {
            jobs,
            rustc,
            rustc_wrapper,
            rustc_workspace_wrapper,
            rustdoc,
            target,
            target_dir,
            rustflags,
            rustdocflags,
            incremental,
            dep_info_basedir,
        })
    }
}

// https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/util/config/target.rs
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#target)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct TargetConfig {
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplelinker)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linker: Option<PathBuf>,
    /// [reference (`target.<triple>.runner`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplerunner)
    ///
    /// [reference (`target.<cfg>.runner`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targetcfgrunner)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<PathAndArgs>,
    /// [reference (`target.<triple>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplerustflags)
    ///
    /// [reference (`target.<cfg>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targetcfgrustflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustflags: Option<Rustflags>,
    // TODO: links: https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplelinks
}

impl TargetConfig {
    fn from_unresolved(de: de::TargetConfig, current_dir: Option<&Path>) -> Result<Self> {
        let linker = de.linker.map(|v| v.resolve_as_program_path(current_dir).into_owned());
        let runner = match de.runner {
            Some(v) => Some(PathAndArgs {
                path: v.path.resolve_program(current_dir).into_owned(),
                args: v.args,
            }),
            None => None,
        };
        let rustflags =
            de.rustflags.map(|v| Rustflags { flags: v.flags.into_iter().map(|v| v.val).collect() });
        Ok(Self { linker, runner, rustflags })
    }
}
