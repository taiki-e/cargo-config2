use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    num::NonZeroI32,
    ops,
    path::{Path, PathBuf},
    slice,
};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::de;
#[doc(no_inline)]
pub use crate::de::{Color, Frequency, When};
pub use crate::{
    command::host_triple,
    paths::ConfigPaths,
    resolve::{ResolveContext, TargetTriple},
    value::{Definition, Value},
};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Config {
    // TODO: paths
    /// Command aliases.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#alias)
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub alias: BTreeMap<String, StringList>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BuildConfig::is_none")]
    pub build: BuildConfig,
    #[serde(default)]
    #[serde(skip_serializing_if = "DocConfig::is_none")]
    pub doc: DocConfig,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, Env>,
    #[serde(default)]
    #[serde(skip_serializing_if = "FutureIncompatReportConfig::is_none")]
    pub future_incompat_report: FutureIncompatReportConfig,
    // TODO: cargo-new
    // TODO: http
    // TODO: install
    #[serde(default)]
    #[serde(skip_serializing_if = "NetConfig::is_none")]
    pub net: NetConfig,
    // TODO: patch
    // TODO: profile
    // TODO: registries
    // TODO: registry
    // TODO: source
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub target: BTreeMap<String, TargetConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "TermConfig::is_none")]
    pub term: TermConfig,

    // Resolve contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    resolved_targets: BTreeSet<String>,
    #[serde(skip)]
    resolve_context: Option<ResolveContext>,
}

impl Config {
    pub(crate) fn from_unresolved(
        mut de: de::Config,
        targets: &[TargetTriple],
        mut cx: ResolveContext,
    ) -> Result<Self> {
        let mut resolved = Self::default();
        de.apply_env(&mut cx)?;

        if let Some(alias) = de.alias {
            for (k, v) in alias {
                resolved.alias.insert(k, v.into_easy_string_list());
            }
        }
        if let Some(build) = de.build {
            resolved.build =
                BuildConfig::from_unresolved(build, de.current_dir.as_deref(), &mut cx)?;
        }
        if let Some(doc) = de.doc {
            resolved.doc.from_unresolved(doc, de.current_dir.as_deref(), &mut cx)?;
        }
        if let Some(env) = de.env {
            for (k, v) in env {
                // TODO
            }
        }
        if let Some(future_incompat_report) = de.future_incompat_report {
            resolved.future_incompat_report.from_unresolved(
                future_incompat_report,
                de.current_dir.as_deref(),
                &mut cx,
            )?;
        }
        if let Some(net) = de.net {
            resolved.net.from_unresolved(net, de.current_dir.as_deref(), &mut cx)?;
        }
        if let Some(term) = de.term {
            resolved.term.from_unresolved(term, de.current_dir.as_deref(), &mut cx)?;
        }
        if let Some(target) = de.target {
            for (k, v) in target {
                // TODO
            }
        }

        // TODO: target

        resolved.resolve_context = Some(cx);
        Ok(resolved)
    }

    /// Read config files hierarchically from the current directory and merges them.
    #[cfg(feature = "toml")]
    #[cfg_attr(docsrs, doc(cfg(feature = "toml")))]
    pub fn load() -> Result<Self> {
        let cx = ResolveContext::new()?;
        let de = de::toml::read_hierarchical(&std::env::current_dir()?)?.unwrap_or_default();
        Self::from_unresolved(de, &[], cx)
    }

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
    pub fn build_target_for_config(
        &self,
        targets: impl IntoIterator<Item = impl Into<TargetTriple>>,
        host: impl Into<TargetTriple>,
    ) -> Result<Vec<TargetTriple>> {
        let targets: Vec<_> = targets.into_iter().map(Into::into).collect();
        if !targets.is_empty() {
            return Ok(targets);
        }
        let config_targets = self.build.target.clone().unwrap_or_default();
        if !config_targets.is_empty() {
            return Ok(config_targets);
        }
        Ok(vec![host.into()])
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
        let config_targets = self.build.target.as_deref().unwrap_or_default();
        if !config_targets.is_empty() {
            return Ok(config_targets
                .iter()
                .map(|t| t.spec_path.as_ref().unwrap_or(&t.triple).clone())
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

impl<T: Borrow<TargetTriple>> ops::Index<T> for Config {
    type Output = TargetConfig;
    fn index(&self, index: T) -> &Self::Output {
        &self.target[&index.borrow().triple]
    }
}

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

    // Resolve contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    override_target_rustflags: bool,
}

impl BuildConfig {
    fn from_unresolved(
        mut de: de::BuildConfig,
        current_dir: Option<&Path>,
        cx: &mut ResolveContext,
    ) -> Result<Self> {
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
                .map(|v| TargetTriple::new(&v.val, v.definition.as_ref(), current_dir))
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
    fn from_unresolved(
        mut de: de::TargetConfig,
        current_dir: Option<&Path>,
        cx: &mut ResolveContext,
    ) -> Result<Self> {
        let linker = de.linker.map(|v| v.resolve_as_program_path(current_dir).into_owned());
        let runner = match de.runner {
            Some(v) => {
                let (path, args) = v.resolve_as_program_path_with_args(current_dir)?;
                Some(PathAndArgs {
                    path: path.into_owned(),
                    args: args.into_iter().map(str::to_owned).collect(),
                })
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
    fn from_unresolved(
        &mut self,
        mut de: de::DocConfig,
        current_dir: Option<&Path>,
        cx: &mut ResolveContext,
    ) -> Result<()> {
        if let Some(v) = de.browser {
            let (path, args) = v.resolve_as_program_path_with_args(current_dir)?;
            self.browser = Some(PathAndArgs {
                path: path.into_owned(),
                args: args.into_iter().map(str::to_owned).collect(),
            });
        }
        Ok(())
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
    fn from_unresolved(
        &mut self,
        mut de: de::FutureIncompatReportConfig,
        current_dir: Option<&Path>,
        cx: &mut ResolveContext,
    ) -> Result<()> {
        self.frequency = de.frequency.map(|v| v.val);
        Ok(())
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
    fn from_unresolved(
        &mut self,
        mut de: de::NetConfig,
        current_dir: Option<&Path>,
        cx: &mut ResolveContext,
    ) -> Result<()> {
        self.retry = de.retry.map(|v| v.val);
        self.git_fetch_with_cli = de.git_fetch_with_cli.map(|v| v.val);
        self.offline = de.offline.map(|v| v.val);
        Ok(())
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
    #[serde(skip_serializing_if = "TermProgress::is_none")]
    pub progress: TermProgress,
}

impl TermConfig {
    fn from_unresolved(
        &mut self,
        mut de: de::TermConfig,
        current_dir: Option<&Path>,
        cx: &mut ResolveContext,
    ) -> Result<()> {
        self.quiet = de.quiet.map(|v| v.val);
        self.verbose = de.verbose.map(|v| v.val);
        self.color = de.color.map(|v| v.val);
        if let Some(progress) = de.progress {
            self.progress.from_unresolved(progress, current_dir, cx)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct TermProgress {
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

impl TermProgress {
    fn from_unresolved(
        &mut self,
        mut de: de::TermProgress,
        current_dir: Option<&Path>,
        cx: &mut ResolveContext,
    ) -> Result<()> {
        self.when = de.when.map(|v| v.val);
        self.width = de.width.map(|v| v.val);
        Ok(())
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
        Self { flags: s.split('\x1f').map(str::to_owned).collect() }
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
        todo!()
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

/// A string or array of strings.
#[allow(clippy::exhaustive_enums)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray<T = String> {
    String(T),
    Array(Vec<T>),
}

impl StringOrArray {
    // /// Flattens an array of strings into a single string.
    // ///
    // /// If this value is a [string](StringOrArray::String), borrows the value as is.
    // pub fn join_array(&self, sep: &str) -> Cow<'_, str> {
    //     match self {
    //         Self::String(s) => Cow::Borrowed(s),
    //         Self::Array(v) => Cow::Owned(v.join(sep)),
    //     }
    // }

    // fn into_string(self) -> String {
    //     match self {
    //         Self::String(s) => s,
    //         Self::Array(v) => v.join(" "),
    //     }
    // }
    // fn to_string(&self) -> Cow<'_, str> {
    //     match self {
    //         Self::String(s) => Cow::Borrowed(s),
    //         Self::Array(v) => Cow::Owned(v.join(" ")),
    //     }
    // }

    // /// Splits a string into a single string.
    // ///
    // /// If this value is an [array](StringOrArray::Array), borrows the value as is.
    // pub fn split_string(&self, pat: char) -> impl Iterator<Item = &str> + '_ {
    //     enum SplitString<'a> {
    //         String(core::str::Split<'a, char>),
    //         Array(core::slice::Iter<'a, String>),
    //     }
    //     impl<'a> Iterator for SplitString<'a> {
    //         type Item = &'a str;
    //         fn next(&mut self) -> Option<Self::Item> {
    //             match self {
    //                 Self::String(s) => s.next(),
    //                 Self::Array(v) => v.next().map(String::as_str),
    //             }
    //         }
    //         fn size_hint(&self) -> (usize, Option<usize>) {
    //             match self {
    //                 Self::String(s) => s.size_hint(),
    //                 Self::Array(v) => v.size_hint(),
    //             }
    //         }
    //     }
    //     match self {
    //         Self::String(s) => SplitString::String(s.split(pat)),
    //         Self::Array(v) => SplitString::Array(v.iter()),
    //     }
    // }
}
impl<T> StringOrArray<T> {
    pub fn string(&self) -> Option<&T> {
        match self {
            Self::String(s) => Some(s),
            Self::Array(_) => None,
        }
    }
    pub fn array(&self) -> Option<&[T]> {
        match self {
            Self::String(_) => None,
            Self::Array(v) => Some(v),
        }
    }
    fn as_array_no_split(&self) -> &[T] {
        match self {
            Self::String(s) => slice::from_ref(s),
            Self::Array(v) => v,
        }
    }
}

impl From<String> for StringOrArray {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<&str> for StringOrArray {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}
impl From<Vec<String>> for StringOrArray {
    fn from(value: Vec<String>) -> Self {
        Self::Array(value)
    }
}
impl From<&[String]> for StringOrArray {
    fn from(value: &[String]) -> Self {
        Self::Array(value.to_owned())
    }
}
impl From<&[&str]> for StringOrArray {
    fn from(value: &[&str]) -> Self {
        Self::Array(value.iter().map(|&v| v.to_owned()).collect())
    }
}
impl<const N: usize> From<[String; N]> for StringOrArray {
    fn from(value: [String; N]) -> Self {
        Self::Array(value[..].to_owned())
    }
}
impl<const N: usize> From<[&str; N]> for StringOrArray {
    fn from(value: [&str; N]) -> Self {
        Self::Array(value[..].iter().map(|&v| v.to_owned()).collect())
    }
}

fn target_u_lower(target: &str) -> String {
    target.replace(['-', '.'], "_")
}
fn target_u_upper(target: &str) -> String {
    let mut target = target_u_lower(target);
    target.make_ascii_uppercase();
    target
}

fn split_space_separated(s: &str) -> impl Iterator<Item = &str> {
    s.split(' ').map(str::trim).filter(|s| !s.is_empty())
}
