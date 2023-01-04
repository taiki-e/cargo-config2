use std::{
    borrow::Borrow,
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
    num::NonZeroI32,
    ops,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use once_cell::unsync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};

use crate::de::{self, split_encoded, split_space_separated};
#[doc(no_inline)]
pub use crate::de::{Color, Frequency, When};
pub use crate::{
    command::host_triple,
    paths::ConfigPaths,
    resolve::{ResolveContext, TargetTriple},
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
    env: OnceCell<BTreeMap<String, Env>>,
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
    target: RefCell<BTreeMap<TargetTriple<'static>, TargetConfig>>,
    term: OnceCell<TermConfig>,

    // Resolve contexts.
    resolved_targets: HashSet<String>,
    resolve_context: ResolveContext,
    current_dir: PathBuf,
}

struct UnresolvedConfig {
    alias: Cell<BTreeMap<String, de::StringList>>,
    build: Cell<de::BuildConfig>,
    doc: Cell<de::DocConfig>,
    env: Cell<BTreeMap<String, de::Env>>,
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
            resolved_targets: HashSet::default(),
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
    pub fn env(&self) -> &BTreeMap<String, Env> {
        self.env.get_or_init(|| {
            let mut map: BTreeMap<String, Env> = BTreeMap::default();
            for (k, v) in self.de.env.take() {
                // TODO
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
    pub fn target(&self, target: &TargetTriple<'_>) -> Result<TargetConfig> {
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
    pub fn build_target_for_config<'a, 'b>(
        &self,
        targets: impl IntoIterator<Item = impl Into<TargetTriple<'a>>>,
        host: impl Into<TargetTriple<'b>>,
    ) -> Result<Vec<TargetTriple<'static>>> {
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
    pub target: Option<Vec<TargetTriple<'static>>>,
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

    // Resolve contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    override_target_rustflags: bool,
}

impl BuildConfig {
    pub(crate) fn from_unresolved(de: de::BuildConfig, current_dir: Option<&Path>) -> Result<Self> {
        let jobs = de.jobs.map(|v| v.val);
        let rustc = de.rustc.map(|v| v.resolve_as_program_path(current_dir));
        let rustc_wrapper = de.rustc_wrapper.map(|v| v.resolve_as_program_path(current_dir));
        let rustc_workspace_wrapper =
            de.rustc_workspace_wrapper.map(|v| v.resolve_as_program_path(current_dir));
        let rustdoc = de.rustdoc.map(|v| v.resolve_as_program_path(current_dir));
        let target = de.target.map(|t| {
            t.as_array_no_split()
                .iter()
                .map(|v| {
                    TargetTriple::new(v.val.clone().into(), v.definition.as_ref(), current_dir)
                })
                .collect()
        });
        let target_dir = de.target_dir.map(|v| v.resolve_as_path(current_dir));
        let rustflags =
            de.rustflags.map(|v| Rustflags { flags: v.flags.into_iter().map(|v| v.val).collect() });
        let rustdocflags = de
            .rustdocflags
            .map(|v| Rustflags { flags: v.flags.into_iter().map(|v| v.val).collect() });
        let incremental = de.incremental.map(|v| v.val);
        let dep_info_basedir = de.dep_info_basedir.map(|v| v.resolve_as_path(current_dir));
        let override_target_rustflags = de.override_target_rustflags;
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
            override_target_rustflags,
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
        let linker = de.linker.map(|v| v.resolve_as_program_path(current_dir));
        let runner = match de.runner {
            Some(v) => {
                Some(PathAndArgs { path: v.path.resolve_program(current_dir), args: v.args })
            }
            None => None,
        };
        let rustflags =
            de.rustflags.map(|v| Rustflags { flags: v.flags.into_iter().map(|v| v.val).collect() });
        Ok(Self { linker, runner, rustflags })
    }
}

/// The `[doc]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#doc)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct DocConfig {
    /// This option sets the browser to be used by `cargo doc`, overriding the
    /// `BROWSER` environment variable when opening documentation with the `--open` option.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#docbrowser)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<PathAndArgs>,
}

impl DocConfig {
    fn from_unresolved(de: de::DocConfig, current_dir: Option<&Path>) -> Result<Self> {
        let browser = de.browser.as_ref().map(|v| PathAndArgs {
            path: v.path.resolve_program(current_dir),
            args: v.args.clone(),
        });
        Ok(Self { browser })
    }
}

/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#env)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Env {
    pub value: String,
    pub force: bool,
    pub relative: bool,
}

impl Serialize for Env {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum EnvRepr<'a> {
            Value(&'a str),
            Table {
                value: &'a str,
                #[serde(skip_serializing_if = "ops::Not::not")]
                force: bool,
                #[serde(skip_serializing_if = "ops::Not::not")]
                relative: bool,
            },
        }
        match self {
            Env { value, force: false, relative: false } => {
                EnvRepr::Value(value).serialize(serializer)
            }
            Env { value, force, relative, .. } => {
                EnvRepr::Table { value, force: *force, relative: *relative }.serialize(serializer)
            }
        }
    }
}
impl<'de> Deserialize<'de> for Env {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum EnvRepr {
            Value(String),
            Table {
                value: String,
                #[serde(default)]
                force: bool,
                #[serde(default)]
                relative: bool,
            },
        }
        match EnvRepr::deserialize(deserializer)? {
            EnvRepr::Value(value) => Ok(Self { value, force: false, relative: false }),
            EnvRepr::Table { value, force, relative } => Ok(Self { value, force, relative }),
        }
    }
}

/// The `[future-incompat-report]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#future-incompat-report)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct FutureIncompatReportConfig {
    /// Controls how often we display a notification to the terminal when a future incompat report is available.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#future-incompat-reportfrequency)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<Frequency>,
}

impl FutureIncompatReportConfig {
    fn from_unresolved(de: de::FutureIncompatReportConfig) -> Result<Self> {
        let frequency = de.frequency.map(|v| v.val);
        Ok(Self { frequency })
    }
}

/// The `[net]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#net)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct NetConfig {
    /// Number of times to retry possibly spurious network errors.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#netretry)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
    /// If this is `true`, then Cargo will use the `git` executable to fetch
    /// registry indexes and git dependencies. If `false`, then it uses a
    /// built-in `git` library.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#netgit-fetch-with-cli)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_fetch_with_cli: Option<bool>,
    /// If this is `true`, then Cargo will avoid accessing the network, and
    /// attempt to proceed with locally cached data. If `false`, Cargo will
    /// access the network as needed, and generate an error if it encounters a
    /// network error.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#netoffline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline: Option<bool>,
}

impl NetConfig {
    fn from_unresolved(de: de::NetConfig) -> Result<Self> {
        let retry = de.retry.map(|v| v.val);
        let git_fetch_with_cli = de.git_fetch_with_cli.map(|v| v.val);
        let offline = de.offline.map(|v| v.val);
        Ok(Self { retry, git_fetch_with_cli, offline })
    }
}

/// The `[term]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#term)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct TermConfig {
    /// Controls whether or not log messages are displayed by Cargo.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termquiet)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet: Option<bool>,
    /// Controls whether or not extra detailed messages are displayed by Cargo.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termverbose)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose: Option<bool>,
    /// Controls whether or not colored output is used in the terminal.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termcolor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default)]
    #[serde(skip_serializing_if = "TermProgressConfig::is_none")]
    pub progress: TermProgressConfig,
}

impl TermConfig {
    fn from_unresolved(de: de::TermConfig) -> Self {
        let quiet = de.quiet.map(|v| v.val);
        let verbose = de.verbose.map(|v| v.val);
        let color = de.color.map(|v| v.val);
        let progress = TermProgressConfig::from_unresolved(de.progress);
        Self { quiet, verbose, color, progress }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct TermProgressConfig {
    /// Controls whether or not progress bar is shown in the terminal.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswhen)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<When>,
    /// Sets the width for progress bar.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswidth)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

impl TermProgressConfig {
    fn from_unresolved(mut de: de::TermProgress) -> Self {
        let when = de.when.map(|v| v.val);
        let width = de.width.map(|v| v.val);
        Self { when, width }
    }
}

/// A representation of rustflags and rustdocflags.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
#[non_exhaustive]
pub struct Rustflags {
    pub flags: Vec<String>,
}

impl Rustflags {
    /// Creates a rustflags from a string separated with ASCII unit separator ('\x1f').
    ///
    /// This is a valid format for the following environment variables:
    ///
    /// - `CARGO_ENCODED_RUSTFLAGS` (Cargo 1.55+)
    /// - `CARGO_ENCODED_RUSTDOCFLAGS` (Cargo 1.55+)
    ///
    /// See also [`encode`](Self::encode).
    pub fn from_encoded(s: &str) -> Self {
        Self { flags: split_encoded(s).map(str::to_owned).collect() }
    }

    /// Creates a rustflags from a string separated with space (' ').
    ///
    /// This is a valid format for the following environment variables:
    ///
    /// - `RUSTFLAGS`
    /// - `CARGO_TARGET_<triple>_RUSTFLAGS`
    /// - `CARGO_BUILD_RUSTFLAGS`
    /// - `RUSTDOCFLAGS`
    /// - `CARGO_BUILD_RUSTDOCFLAGS`
    ///
    /// And the following configs:
    ///
    /// - `target.<triple>.rustflags`
    /// - `target.<cfg>.rustflags`
    /// - `build.rustflags`
    /// - `build.rustdocflags`
    ///
    /// See also [`encode_space_separated`](Self::encode_space_separated).
    pub fn from_space_separated(s: &str) -> Self {
        Self { flags: split_space_separated(s).map(str::to_owned).collect() }
    }

    /// Concatenates this rustflags with ASCII unit separator ('\x1f').
    ///
    /// This is a valid format for the following environment variables:
    ///
    /// - `CARGO_ENCODED_RUSTFLAGS` (Cargo 1.55+)
    /// - `CARGO_ENCODED_RUSTDOCFLAGS` (Cargo 1.55+)
    ///
    /// # Errors
    ///
    /// This returns an error if any of flag contains ASCII unit separator ('\x1f').
    ///
    /// This is because even if you do not intend it to be interpreted as a
    /// separator, Cargo will interpret it as a separator.
    ///
    /// Since it is not easy to insert an ASCII unit separator in a toml file or
    /// Shell environment variable, this usually occurs when this rustflags is
    /// created in the wrong way ([`from_encoded`](Self::from_encoded) vs
    /// [`from_space_separated`](Self::from_space_separated)) or when a flag
    /// containing a separator is written in the rust code ([`push`](Self::push),
    /// `into`, `from`, etc.).
    pub fn encode(&self) -> Result<String> {
        self.encode_internal('\x1f')
    }

    /// Concatenates this rustflags with space (' ').
    ///
    /// This is a valid format for the following environment variables:
    ///
    /// - `RUSTFLAGS`
    /// - `CARGO_TARGET_<triple>_RUSTFLAGS`
    /// - `CARGO_BUILD_RUSTFLAGS`
    /// - `RUSTDOCFLAGS`
    /// - `CARGO_BUILD_RUSTDOCFLAGS`
    ///
    /// And the following configs:
    ///
    /// - `target.<triple>.rustflags`
    /// - `target.<cfg>.rustflags`
    /// - `build.rustflags`
    /// - `build.rustdocflags`
    ///
    /// # Errors
    ///
    /// This returns an error if any of flag contains space (' ').
    ///
    /// This is because even if you do not intend it to be interpreted as a
    /// separator, Cargo will interpret it as a separator.
    ///
    /// If you run into this error, consider using a more robust
    /// [`CARGO_ENCODED_*` flags](Self::encode).
    pub fn encode_space_separated(&self) -> Result<String> {
        self.encode_internal(' ')
    }

    fn encode_internal(&self, sep: char) -> Result<String> {
        let mut buf = String::with_capacity(
            self.flags.len().saturating_sub(1) + self.flags.iter().map(String::len).sum::<usize>(),
        );
        for flag in &self.flags {
            if flag.contains(sep) {
                bail!("flag in rustflags must not contain its separator ({sep:?})");
            }
            if !buf.is_empty() {
                buf.push(sep);
            }
            buf.push_str(flag);
        }
        Ok(buf)
    }

    /// Appends a flag to the back of this rustflags.
    pub fn push(&mut self, flag: impl Into<String>) {
        self.flags.push(flag.into());
    }
}

impl PartialEq for Rustflags {
    fn eq(&self, other: &Self) -> bool {
        self.flags == other.flags
    }
}

impl<'de> Deserialize<'de> for Rustflags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v: StringOrArray = Deserialize::deserialize(deserializer)?;
        match v {
            StringOrArray::String(s) => Ok(Self::from_space_separated(&s)),
            StringOrArray::Array(v) => Ok(Self { flags: v }),
        }
    }
}

impl From<Vec<String>> for Rustflags {
    fn from(value: Vec<String>) -> Self {
        Self { flags: value }
    }
}
impl From<&[String]> for Rustflags {
    fn from(value: &[String]) -> Self {
        Self { flags: value.to_owned() }
    }
}
impl From<&[&str]> for Rustflags {
    fn from(value: &[&str]) -> Self {
        Self { flags: value.iter().map(|&v| v.to_owned()).collect() }
    }
}
impl<const N: usize> From<[String; N]> for Rustflags {
    fn from(value: [String; N]) -> Self {
        Self { flags: value[..].to_owned() }
    }
}
impl<const N: usize> From<[&str; N]> for Rustflags {
    fn from(value: [&str; N]) -> Self {
        Self { flags: value[..].iter().map(|&v| v.to_owned()).collect() }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PathAndArgs {
    pub path: PathBuf,
    pub args: Vec<String>,
}

impl Serialize for PathAndArgs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        todo!()
    }
}
impl<'de> Deserialize<'de> for PathAndArgs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let v: StringOrArray = Deserialize::deserialize(deserializer)?;
        match v {
            StringOrArray::String(s) => {
                let mut s = split_space_separated(&s);
                let path = match s.next() {
                    Some(path) => path,
                    None => return Err(D::Error::invalid_length(0, &"at least one element")),
                };
                Ok(Self { path: path.into(), args: s.map(str::to_owned).collect() })
            }
            StringOrArray::Array(mut v) => {
                if v.is_empty() {
                    return Err(D::Error::invalid_length(0, &"at least one element"));
                }
                let path = v.remove(0);
                Ok(Self { path: path.into(), args: v })
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
#[non_exhaustive]
pub struct StringList {
    pub list: Vec<String>,
}

impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v: StringOrArray = Deserialize::deserialize(deserializer)?;
        match v {
            StringOrArray::String(s) => {
                Ok(Self { list: split_space_separated(&s).map(str::to_owned).collect() })
            }
            StringOrArray::Array(v) => Ok(Self { list: v }),
        }
    }
}

impl From<de::StringList> for StringList {
    fn from(value: de::StringList) -> Self {
        Self { list: value.list.into_iter().map(|v| v.val).collect() }
    }
}

/// A string or array of strings.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum StringOrArray {
    String(String),
    Array(Vec<String>),
}
