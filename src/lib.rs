/*!
Structured access to [Cargo configuration](https://doc.rust-lang.org/nightly/cargo/reference/config.html).

This library is intended to accurately emulate the actual behavior of Cargo configuration, for example, this supports the following behaviors:

- [Hierarchical structure and merge](https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure)
- Environment variables resolution.
- `target.<triple>` and `target.<cfg>` resolution.

Supported tables and fields are mainly based on [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)'s use cases, but feel free to submit an issue if you see something missing in your use case.

## Examples

```no_run
# fn main() -> anyhow::Result<()> {
// Read config files hierarchically from the current directory and merges them.
let mut config = cargo_config2::Config::load()?;
// Apply environment variables and resolves target-specific configuration
// (`target.<triple>` and `target.<cfg>`).
let target = "x86_64-unknown-linux-gnu";
config.resolve(target)?;
// Display resolved rustflags for `target`.
println!("{:?}", config.target[target].rustflags);
# Ok(()) }
```

See also the [`get` example](https://github.com/taiki-e/cargo-config2/blob/HEAD/examples/get.rs) that partial re-implementation of `cargo config get` using cargo-config2.
*/

#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms, single_use_lifetimes),
        allow(dead_code, unused_variables)
    )
))]
#![forbid(unsafe_code)]
#![warn(
    missing_debug_implementations,
    // missing_docs,
    rust_2018_idioms,
    single_use_lifetimes,
    unreachable_pub
)]
#![warn(
    clippy::pedantic,
    // lints for public library
    // clippy::alloc_instead_of_core,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    // clippy::std_instead_of_alloc,
    // clippy::std_instead_of_core,
)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::single_match_else,
    clippy::unnecessary_wraps
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Refs:
// - https://doc.rust-lang.org/nightly/cargo/reference/config.html

#[cfg(test)]
#[path = "gen/assert_impl.rs"]
mod assert_impl;
#[path = "gen/is_none.rs"]
mod is_none_impl;
#[path = "gen/merge.rs"]
mod merge_impl;

#[macro_use]
mod process;

pub mod api;
mod command;
pub mod de;
pub mod easy;
mod env;
mod merge;
mod paths;
mod resolve;
#[cfg(feature = "toml")]
#[cfg_attr(docsrs, doc(cfg(feature = "toml")))]
pub mod toml;
mod value;

use std::{
    borrow::{Borrow, Cow},
    collections::{BTreeMap, BTreeSet},
    num::NonZeroI32,
    ops,
    path::{Path, PathBuf},
    slice,
};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[doc(no_inline)]
pub use crate::de::{Color, Frequency, When};
use crate::value::SetPath;
pub use crate::{
    command::host_triple,
    paths::ConfigPaths,
    resolve::{ResolveContext, TargetTriple},
    value::{Definition, Value},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Config {
    // TODO: paths
    /// Command aliases.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#alias)
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub alias: BTreeMap<String, StringOrArray>,
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

    // Load contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    current_dir: Option<PathBuf>,
    #[serde(skip)]
    path: Option<PathBuf>,
    // Resolve contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    env_applied: bool,
    #[serde(skip)]
    resolved_targets: BTreeSet<String>,
}

impl Config {
    /// Read config files hierarchically from the current directory and merges them.
    #[cfg(feature = "toml")]
    #[cfg_attr(docsrs, doc(cfg(feature = "toml")))]
    pub fn load() -> Result<Self> {
        Self::load_with_cwd(std::env::current_dir()?)
    }

    /// Read config files hierarchically from the given directory and merges them.
    #[cfg(feature = "toml")]
    #[cfg_attr(docsrs, doc(cfg(feature = "toml")))]
    pub fn load_with_cwd(current_dir: impl AsRef<Path>) -> Result<Self> {
        Ok(toml::read_hierarchical(current_dir.as_ref())?.unwrap_or_default())
    }

    /// Read config files hierarchically from the current directory.
    #[cfg(feature = "toml")]
    #[cfg_attr(docsrs, doc(cfg(feature = "toml")))]
    pub fn load_unmerged() -> Result<Vec<Self>> {
        Self::load_unmerged_with_cwd(std::env::current_dir()?)
    }

    /// Read config files hierarchically from the given directory.
    #[cfg(feature = "toml")]
    #[cfg_attr(docsrs, doc(cfg(feature = "toml")))]
    pub fn load_unmerged_with_cwd(current_dir: impl AsRef<Path>) -> Result<Vec<Self>> {
        toml::read_hierarchical_unmerged(current_dir.as_ref())
    }

    /// Applies environment variables and resolves target-specific configuration (`target.<triple>` and `target.<cfg>`).
    pub fn resolve(&mut self, target: impl Into<TargetTriple>) -> Result<()> {
        let cx = &mut ResolveContext::new()?;
        self.resolve_with_context(cx, target.into())
    }

    /// Applies environment variables and resolves target-specific configuration (`target.<triple>` and `target.<cfg>`).
    pub fn resolve_with_context(
        &mut self,
        cx: &mut ResolveContext,
        target: impl Into<TargetTriple>,
    ) -> Result<()> {
        self.resolve_env(cx)?;
        self.resolve_target(cx, &target.into())?;
        Ok(())
    }

    fn resolve_env(&mut self, cx: &mut ResolveContext) -> Result<()> {
        if !self.env_applied {
            self.apply_env(cx)?;
            self.env_applied = true;
        }
        Ok(())
    }

    fn resolve_target(
        &mut self,
        cx: &mut ResolveContext,
        target_triple: &TargetTriple,
    ) -> Result<()> {
        let target = &target_triple.triple;

        // In target rustflags, all occurrences are merged, so we need to avoid multiple calls.
        if self.resolved_targets.contains(target) {
            return Ok(());
        }
        self.resolved_targets.insert(target.clone());

        let target_u_upper = target_u_upper(target);
        let mut target_config = self.target.remove(target).unwrap_or_default();
        let mut target_linker = target_config.linker.take();
        let mut target_runner = target_config.runner.take();
        let mut target_rustflags: Option<Rustflags> = target_config.rustflags.take();
        if let Some(linker) = cx.env_dyn(&format!("CARGO_TARGET_{target_u_upper}_LINKER"))? {
            target_linker = Some(linker);
        }
        // Priorities (as of 1.68.0-nightly (2022-12-23)):
        // 1. CARGO_TARGET_<triple>_RUNNER
        // 2. target.<triple>.runner
        // 3. target.<cfg>.runner
        if let Some(runner) = cx.env_dyn(&format!("CARGO_TARGET_{target_u_upper}_RUNNER"))? {
            target_runner = Some(StringOrArray::String(runner));
        }
        // Applied order (as of 1.68.0-nightly (2022-12-23)):
        // 1. target.<triple>.rustflags
        // 2. CARGO_TARGET_<triple>_RUSTFLAGS
        // 3. target.<cfg>.rustflags
        if let Some(rustflags) = cx.env_dyn(&format!("CARGO_TARGET_{target_u_upper}_RUSTFLAGS"))? {
            target_rustflags.get_or_insert_with(Rustflags::default).flags.push(rustflags.val);
        }
        for (k, v) in &self.target {
            if cx.eval_cfg(k, target_triple)? {
                if target_runner.is_none() {
                    if let Some(runner) = v.runner.as_ref() {
                        target_runner = Some(runner.clone());
                    }
                }
                if let Some(rustflags) = v.rustflags.as_ref() {
                    target_rustflags
                        .get_or_insert_with(Rustflags::default)
                        .flags
                        .extend_from_slice(&rustflags.flags);
                }
            }
        }
        if let Some(linker) = target_linker {
            target_config.linker = Some(linker);
        }
        if let Some(runner) = target_runner {
            target_config.runner = Some(runner);
        }
        if self.build.override_target_rustflags {
            target_config.rustflags = self.build.rustflags.clone();
        } else if let Some(rustflags) = target_rustflags {
            target_config.rustflags = Some(rustflags);
        } else {
            target_config.rustflags = self.build.rustflags.clone();
        }
        self.target.insert(target.clone(), target_config);
        Ok(())
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
        if !self.env_applied {
            if let Some(target) = env::var("CARGO_BUILD_TARGET")? {
                return Ok(vec![target.into()]);
            }
        }
        let config_targets =
            self.build.target.as_ref().map(StringOrArray::as_array_no_split).unwrap_or_default();
        if !config_targets.is_empty() {
            return Ok(config_targets
                .iter()
                .map(|v| {
                    TargetTriple::new(&v.val, v.definition.as_ref(), self.current_dir.as_deref())
                })
                .collect());
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
        if !self.env_applied {
            if let Some(target) = env::var("CARGO_BUILD_TARGET")? {
                return Ok(vec![target]);
            }
        }
        let config_targets =
            self.build.target.as_ref().map(StringOrArray::as_array_no_split).unwrap_or_default();
        if !config_targets.is_empty() {
            return Ok(config_targets
                .iter()
                .map(|v| {
                    let t = TargetTriple::new(
                        &v.val,
                        v.definition.as_ref(),
                        self.current_dir.as_deref(),
                    );
                    t.spec_path.unwrap_or(t.triple)
                })
                .collect());
        }
        Ok(vec![])
    }

    /// Merges the given config into this config.
    ///
    /// If `force` is `false`, this matches the way cargo [merges configs in the
    /// parent directories](https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure).
    ///
    /// If `force` is `true`, this matches the way cargo's `--config` CLI option
    /// overrides config.
    pub fn merge(&mut self, from: Self, force: bool) -> Result<()> {
        merge::Merge::merge(self, from, force)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn set_cwd(&mut self, path: PathBuf) {
        // for alias in self.alias.values_mut() {
        //     alias.set_cwd(&path);
        // }
        self.build.set_cwd(&path);
        self.doc.set_cwd(&path);
        // for env in self.env.values_mut() {
        //     env.set_cwd(&path);
        // }
        // self.future_incompat_report.set_cwd(&path);
        // self.net.set_cwd(&path);
        for target in self.target.values_mut() {
            target.set_cwd(&path);
        }
        // self.term.set_cwd(&path);
        self.current_dir = Some(path);
    }
    fn set_path(&mut self, path: PathBuf) {
        // for alias in self.alias.values_mut() {
        //     alias.set_path(&path);
        // }
        self.build.set_path(&path);
        self.doc.set_path(&path);
        for env in self.env.values_mut() {
            env.set_path(&path);
        }
        // self.future_incompat_report.set_path(&path);
        // self.net.set_path(&path);
        for target in self.target.values_mut() {
            target.set_path(&path);
        }
        // self.term.set_path(&path);
        self.path = Some(path);
    }
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
    pub rustc: Option<Value<String>>,
    /// Sets a wrapper to execute instead of `rustc`.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-wrapper)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustc_wrapper: Option<Value<String>>,
    /// Sets a wrapper to execute instead of `rustc`, for workspace members only.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-workspace-wrapper)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustc_workspace_wrapper: Option<Value<String>>,
    /// Sets the executable to use for `rustdoc`.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdoc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustdoc: Option<Value<String>>,
    /// The default target platform triples to compile to.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<StringOrArray<Value<String>>>,
    /// The path to where all compiler output is placed. The default if not
    /// specified is a directory named target located at the root of the workspace.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_dir: Option<Value<String>>,
    /// Extra command-line flags to pass to rustc. The value may be an array
    /// of strings or a space-separated string.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustflags: Option<Rustflags>,
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
    pub dep_info_basedir: Option<Value<String>>,

    // Load contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    current_dir: Option<PathBuf>,
    // Resolve contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    override_target_rustflags: bool,
}

impl BuildConfig {
    pub fn rustc(&self) -> Option<Cow<'_, Path>> {
        Some(self.rustc.as_ref()?.resolve_as_program_path(self.current_dir.as_deref()))
    }
    pub fn rustc_wrapper(&self) -> Option<Cow<'_, Path>> {
        Some(self.rustc_wrapper.as_ref()?.resolve_as_program_path(self.current_dir.as_deref()))
    }
    pub fn rustc_workspace_wrapper(&self) -> Option<Cow<'_, Path>> {
        Some(
            self.rustc_workspace_wrapper
                .as_ref()?
                .resolve_as_program_path(self.current_dir.as_deref()),
        )
    }
    pub fn rustdoc(&self) -> Option<Cow<'_, Path>> {
        Some(self.rustdoc.as_ref()?.resolve_as_program_path(self.current_dir.as_deref()))
    }
    pub fn target_dir(&self) -> Option<Cow<'_, Path>> {
        Some(self.target_dir.as_ref()?.resolve_as_path(self.current_dir.as_deref()))
    }
    pub fn dep_info_basedir(&self) -> Option<Cow<'_, Path>> {
        Some(self.dep_info_basedir.as_ref()?.resolve_as_path(self.current_dir.as_deref()))
    }

    fn set_cwd(&mut self, path: &Path) {
        self.current_dir = Some(path.to_owned());
    }
    fn set_path(&mut self, path: &Path) {
        // self.jobs.set_path(path);
        self.rustc.set_path(path);
        self.rustc_wrapper.set_path(path);
        self.rustc_workspace_wrapper.set_path(path);
        self.rustdoc.set_path(path);
        self.target.set_path(path);
        self.target_dir.set_path(path);
        // self.rustflags.set_path(&path);
        // self.rustdocflags.set_path(&path);
        // self.incremental.set_path(&path);
        self.dep_info_basedir.set_path(path);
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
    pub linker: Option<Value<String>>,
    /// [reference (`target.<triple>.runner`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplerunner)
    ///
    /// [reference (`target.<cfg>.runner`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targetcfgrunner)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<StringOrArray<Value<String>>>,
    /// [reference (`target.<triple>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplerustflags)
    ///
    /// [reference (`target.<cfg>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targetcfgrustflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustflags: Option<Rustflags>,
    // TODO: links: https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplelinks

    // Load contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    current_dir: Option<PathBuf>,
}

impl TargetConfig {
    pub fn linker(&self) -> Option<Cow<'_, Path>> {
        Some(self.linker.as_ref()?.resolve_as_program_path(self.current_dir.as_deref()))
    }
    pub fn runner(&self) -> Result<Option<(Cow<'_, Path>, Vec<&str>)>> {
        match self.runner.as_ref() {
            Some(runner) => {
                Ok(Some(runner.resolve_as_program_path_with_args(self.current_dir.as_deref())?))
            }
            None => Ok(None),
        }
    }

    fn set_cwd(&mut self, path: &Path) {
        self.current_dir = Some(path.to_owned());
    }
    fn set_path(&mut self, path: &Path) {
        self.linker.set_path(path);
        self.runner.set_path(path);
        // self.rustflags.set_path(path);
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
    pub browser: Option<StringOrArray<Value<String>>>,

    // Load contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    current_dir: Option<PathBuf>,
}

impl DocConfig {
    pub fn browser(&self) -> Result<Option<(Cow<'_, Path>, Vec<&str>)>> {
        match self.browser.as_ref() {
            Some(browser) => {
                Ok(Some(browser.resolve_as_program_path_with_args(self.current_dir.as_deref())?))
            }
            None => Ok(None),
        }
    }

    fn set_cwd(&mut self, path: &Path) {
        self.current_dir = Some(path.to_owned());
    }
    fn set_path(&mut self, path: &Path) {
        self.browser.set_path(path);
    }
}

/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#env)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Env {
    pub value: Value<String>,
    pub force: Option<bool>,
    pub relative: Option<bool>,
    deserialized_repr: EnvDeserializedRepr,
}

#[derive(Debug, Clone, Copy)]
enum EnvDeserializedRepr {
    Value,
    Table,
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
                #[serde(skip_serializing_if = "Option::is_none")]
                force: Option<bool>,
                #[serde(skip_serializing_if = "Option::is_none")]
                relative: Option<bool>,
            },
        }
        match self {
            Env {
                value,
                force: None,
                relative: None,
                deserialized_repr: EnvDeserializedRepr::Value,
            } => EnvRepr::Value(&value.val).serialize(serializer),
            Env { value, force, relative, .. } => {
                EnvRepr::Table { value: &value.val, force: *force, relative: *relative }
                    .serialize(serializer)
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
            Table { value: String, force: Option<bool>, relative: Option<bool> },
        }
        match EnvRepr::deserialize(deserializer)? {
            EnvRepr::Value(value) => Ok(Self {
                value: Value { val: value, definition: None },
                force: None,
                relative: None,
                deserialized_repr: EnvDeserializedRepr::Value,
            }),
            EnvRepr::Table { value, force, relative } => Ok(Self {
                value: Value { val: value, definition: None },
                force,
                relative,
                deserialized_repr: EnvDeserializedRepr::Table,
            }),
        }
    }
}

impl Env {
    fn set_path(&mut self, path: &Path) {
        self.value.set_path(path);
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
    pub color: Option<When>,
    #[serde(default)]
    #[serde(skip_serializing_if = "TermProgress::is_none")]
    pub progress: TermProgress,
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
/// A representation of rustflags and rustdocflags.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct Rustflags {
    flags: Vec<String>,
    // for merge
    #[serde(skip)]
    deserialized_repr: RustflagsDeserializedRepr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RustflagsDeserializedRepr {
    Unknown,
    String,
    Array,
}

impl Default for RustflagsDeserializedRepr {
    fn default() -> Self {
        Self::Unknown
    }
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
        Self {
            flags: s.split('\x1f').map(str::to_owned).collect(),
            deserialized_repr: RustflagsDeserializedRepr::Unknown,
        }
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
        Self {
            flags: split_space_separated(s).map(str::to_owned).collect(),
            deserialized_repr: RustflagsDeserializedRepr::String,
        }
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

impl ops::Deref for Rustflags {
    type Target = [String];
    fn deref(&self) -> &Self::Target {
        &self.flags
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
            StringOrArray::Array(v) => {
                Ok(Self { flags: v, deserialized_repr: RustflagsDeserializedRepr::Array })
            }
        }
    }
}

impl From<Rustflags> for Vec<String> {
    fn from(value: Rustflags) -> Self {
        value.flags
    }
}
impl From<Vec<String>> for Rustflags {
    fn from(value: Vec<String>) -> Self {
        Self { flags: value, deserialized_repr: RustflagsDeserializedRepr::Array }
    }
}
impl From<&[String]> for Rustflags {
    fn from(value: &[String]) -> Self {
        Self { flags: value.to_owned(), deserialized_repr: RustflagsDeserializedRepr::Array }
    }
}
impl From<&[&str]> for Rustflags {
    fn from(value: &[&str]) -> Self {
        Self {
            flags: value.iter().map(|&v| v.to_owned()).collect(),
            deserialized_repr: RustflagsDeserializedRepr::Array,
        }
    }
}
impl<const N: usize> From<[String; N]> for Rustflags {
    fn from(value: [String; N]) -> Self {
        Self { flags: value[..].to_owned(), deserialized_repr: RustflagsDeserializedRepr::Array }
    }
}
impl<const N: usize> From<[&str; N]> for Rustflags {
    fn from(value: [&str; N]) -> Self {
        Self {
            flags: value[..].iter().map(|&v| v.to_owned()).collect(),
            deserialized_repr: RustflagsDeserializedRepr::Array,
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
