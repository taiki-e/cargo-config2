// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cargo configuration that environment variables, config overrides, and
//! target-specific configurations have not been resolved.

#[path = "gen/de.rs"]
mod gen;

use core::{fmt, slice, str::FromStr};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use serde::{
    de::{self, Deserialize, Deserializer},
    ser::{Serialize, Serializer},
};
use serde_derive::{Deserialize, Serialize};

pub use crate::value::{Definition, Value};
use crate::{
    easy,
    error::{Context as _, Error, Result},
    resolve::{ResolveContext, TargetTripleRef},
    walk,
};

/// Cargo configuration that environment variables, config overrides, and
/// target-specific configurations have not been resolved.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Config {
    // TODO: paths
    /// The `[alias]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#alias)
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub alias: BTreeMap<String, StringList>,
    /// The `[build]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#build)
    #[serde(default)]
    #[serde(skip_serializing_if = "BuildConfig::is_none")]
    pub build: BuildConfig,
    /// The `[doc]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#doc)
    #[serde(default)]
    #[serde(skip_serializing_if = "DocConfig::is_none")]
    pub doc: DocConfig,
    /// The `[env]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#env)
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, EnvConfigValue>,
    /// The `[future-incompat-report]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#future-incompat-report)
    #[serde(default)]
    #[serde(skip_serializing_if = "FutureIncompatReportConfig::is_none")]
    pub future_incompat_report: FutureIncompatReportConfig,
    // TODO: cargo-new
    // TODO: http
    // TODO: install
    /// The `[net]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#net)
    #[serde(default)]
    #[serde(skip_serializing_if = "NetConfig::is_none")]
    pub net: NetConfig,
    // TODO: patch
    // TODO: profile
    /// The `[registries]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registries)
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub registries: BTreeMap<String, RegistriesConfigValue>,
    /// The `[registry]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registry)
    #[serde(default)]
    #[serde(skip_serializing_if = "RegistryConfig::is_none")]
    pub registry: RegistryConfig,
    // TODO: source
    /// The `[target]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#target)
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub target: BTreeMap<String, TargetConfig>,
    /// The `[term]` table.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#term)
    #[serde(default)]
    #[serde(skip_serializing_if = "TermConfig::is_none")]
    pub term: TermConfig,
}

impl Config {
    /// Read config files hierarchically from the current directory and merges them.
    pub fn load() -> Result<Self> {
        Self::load_with_cwd(std::env::current_dir().context("failed to get current directory")?)
    }

    /// Read config files hierarchically from the given directory and merges them.
    pub fn load_with_cwd<P: AsRef<Path>>(cwd: P) -> Result<Self> {
        let cwd = cwd.as_ref();
        Self::_load_with_options(cwd, walk::cargo_home_with_cwd(cwd).as_deref())
    }

    /// Read config files hierarchically from the given directory and merges them.
    pub fn load_with_options<P: AsRef<Path>, Q: Into<Option<PathBuf>>>(
        cwd: P,
        cargo_home: Q,
    ) -> Result<Self> {
        Self::_load_with_options(cwd.as_ref(), cargo_home.into().as_deref())
    }
    pub(crate) fn _load_with_options(
        current_dir: &Path,
        cargo_home: Option<&Path>,
    ) -> Result<Config> {
        let mut base = None;
        for path in crate::walk::WalkInner::with_cargo_home(current_dir, cargo_home) {
            let config = Self::_load_file(&path)?;
            match &mut base {
                None => base = Some((path, config)),
                Some((base_path, base)) => base.merge(config, false).with_context(|| {
                    format!(
                        "failed to merge config from `{}` into `{}`",
                        path.display(),
                        base_path.display()
                    )
                })?,
            }
        }
        Ok(base.map(|(_, c)| c).unwrap_or_default())
    }

    /// Reads cargo config file at the given path.
    ///
    /// **Note:** Note: This just reads a file at the given path and does not
    /// respect the hierarchical structure of the cargo config.
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::_load_file(path.as_ref())
    }
    fn _load_file(path: &Path) -> Result<Self> {
        let buf = fs::read_to_string(path)
            .with_context(|| format!("failed to read `{}`", path.display()))?;
        let mut config: Config = toml_edit::de::from_str(&buf).with_context(|| {
            format!("failed to parse `{}` as cargo configuration", path.display())
        })?;
        config.set_path(path);
        Ok(config)
    }

    /// Merges the given config into this config.
    ///
    /// If `force` is `false`, this matches the way cargo [merges configs in the
    /// parent directories](https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure).
    ///
    /// If `force` is `true`, this matches the way cargo's `--config` CLI option
    /// overrides config.
    pub(crate) fn merge(&mut self, low: Self, force: bool) -> Result<()> {
        crate::merge::Merge::merge(self, low, force)
    }

    pub(crate) fn set_path(&mut self, path: &Path) {
        crate::value::SetPath::set_path(self, path);
    }

    pub(crate) fn resolve_target(
        cx: &ResolveContext,
        target_configs: &BTreeMap<String, TargetConfig>,
        override_target_rustflags: bool,
        build_rustflags: &Option<Flags>,
        target_triple: &TargetTripleRef<'_>,
        build_config: &easy::BuildConfig,
    ) -> Result<Option<TargetConfig>> {
        let target = target_triple.triple();
        if target.starts_with("cfg(") {
            bail!("'{target}' is not valid target triple");
        }
        let mut target_config = target_configs.get(target).cloned();

        let target_u_upper = target_u_upper(target);
        let mut target_linker = target_config.as_mut().and_then(|c| c.linker.take());
        let mut target_runner = target_config.as_mut().and_then(|c| c.runner.take());
        let mut target_rustflags: Option<Flags> =
            target_config.as_mut().and_then(|c| c.rustflags.take());
        if let Some(linker) = cx.env_dyn(&format!("CARGO_TARGET_{target_u_upper}_LINKER"))? {
            target_linker = Some(linker);
        }
        if let Some(runner) = cx.env_dyn(&format!("CARGO_TARGET_{target_u_upper}_RUNNER"))? {
            target_runner = Some(
                PathAndArgs::from_string(&runner.val, runner.definition)
                    .context("invalid length 0, expected at least one element")?,
            );
        }
        if let Some(rustflags) = cx.env_dyn(&format!("CARGO_TARGET_{target_u_upper}_RUSTFLAGS"))? {
            let mut rustflags =
                Flags::from_space_separated(&rustflags.val, rustflags.definition.as_ref());
            match &mut target_rustflags {
                Some(target_rustflags) => {
                    target_rustflags.flags.append(&mut rustflags.flags);
                }
                target_rustflags @ None => *target_rustflags = Some(rustflags),
            }
        }
        for (k, v) in target_configs {
            if !k.starts_with("cfg(") {
                continue;
            }
            if cx.eval_cfg(k, target_triple, build_config)? {
                // https://github.com/rust-lang/cargo/pull/12535
                if target_linker.is_none() {
                    if let Some(linker) = v.linker.as_ref() {
                        target_linker = Some(linker.clone());
                    }
                }
                // Priorities (as of 1.68.0-nightly (2022-12-23)):
                // 1. CARGO_TARGET_<triple>_RUNNER
                // 2. target.<triple>.runner
                // 3. target.<cfg>.runner
                if target_runner.is_none() {
                    if let Some(runner) = v.runner.as_ref() {
                        target_runner = Some(runner.clone());
                    }
                }
                // Applied order (as of 1.68.0-nightly (2022-12-23)):
                // 1. target.<triple>.rustflags
                // 2. CARGO_TARGET_<triple>_RUSTFLAGS
                // 3. target.<cfg>.rustflags
                if let Some(rustflags) = v.rustflags.as_ref() {
                    match &mut target_rustflags {
                        Some(target_rustflags) => {
                            target_rustflags.flags.extend_from_slice(&rustflags.flags);
                        }
                        target_rustflags @ None => *target_rustflags = Some(rustflags.clone()),
                    }
                }
            }
        }
        if let Some(linker) = target_linker {
            target_config.get_or_insert_with(TargetConfig::default).linker = Some(linker);
        }
        if let Some(runner) = target_runner {
            target_config.get_or_insert_with(TargetConfig::default).runner = Some(runner);
        }
        if override_target_rustflags {
            target_config
                .get_or_insert_with(TargetConfig::default)
                .rustflags
                .clone_from(build_rustflags);
        } else if let Some(rustflags) = target_rustflags {
            target_config.get_or_insert_with(TargetConfig::default).rustflags = Some(rustflags);
        } else {
            target_config
                .get_or_insert_with(TargetConfig::default)
                .rustflags
                .clone_from(build_rustflags);
        }
        Ok(target_config)
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
    pub jobs: Option<Value<i32>>,
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
    pub target: Option<StringOrArray>,
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
    pub rustflags: Option<Flags>,
    /// Extra command-line flags to pass to `rustdoc`. The value may be an array
    /// of strings or a space-separated string.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustdocflags: Option<Flags>,
    /// Whether or not to perform incremental compilation.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildincremental)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental: Option<Value<bool>>,
    /// Strips the given path prefix from dep info file paths.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#builddep-info-basedir)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dep_info_basedir: Option<Value<String>>,

    // Resolve contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    pub(crate) override_target_rustflags: bool,
}

// https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/util/config/target.rs
/// A `[target.<triple>]` or `[target.<cfg>]` table.
///
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
    pub runner: Option<PathAndArgs>,
    /// [reference (`target.<triple>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplerustflags)
    ///
    /// [reference (`target.<cfg>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targetcfgrustflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustflags: Option<Flags>,
    // TODO: links: https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplelinks
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

// TODO: hide internal repr, change to struct
/// A value of the `[env]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#env)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum EnvConfigValue {
    Value(Value<String>),
    Table {
        value: Value<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        force: Option<Value<bool>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        relative: Option<Value<bool>>,
    },
}

impl EnvConfigValue {
    pub(crate) const fn kind(&self) -> &'static str {
        match self {
            Self::Value(..) => "string",
            Self::Table { .. } => "table",
        }
    }

    pub(crate) fn resolve(&self, current_dir: &Path) -> Cow<'_, OsStr> {
        match self {
            Self::Value(v) => OsStr::new(&v.val).into(),
            Self::Table { value, relative, .. } => {
                if relative.as_ref().map_or(false, |v| v.val) {
                    if let Some(def) = &value.definition {
                        return def.root(current_dir).join(&value.val).into_os_string().into();
                    }
                }
                OsStr::new(&value.val).into()
            }
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
    pub frequency: Option<Value<Frequency>>,
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
    pub retry: Option<Value<u32>>,
    /// If this is `true`, then Cargo will use the `git` executable to fetch
    /// registry indexes and git dependencies. If `false`, then it uses a
    /// built-in `git` library.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#netgit-fetch-with-cli)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_fetch_with_cli: Option<Value<bool>>,
    /// If this is `true`, then Cargo will avoid accessing the network, and
    /// attempt to proceed with locally cached data. If `false`, Cargo will
    /// access the network as needed, and generate an error if it encounters a
    /// network error.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#netoffline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline: Option<Value<bool>>,
}

/// A value of the `[registries]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registries)
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct RegistriesConfigValue {
    /// Specifies the URL of the git index for the registry.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registriesnameindex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<Value<String>>,
    /// Specifies the authentication token for the given registry.
    ///
    /// Note: This library does not read any values in the
    /// [credentials](https://doc.rust-lang.org/nightly/cargo/reference/config.html#credentials)
    /// file.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registriesnametoken)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<Value<String>>,
    /// Specifies the protocol used to access crates.io.
    /// Not allowed for any registries besides crates.io.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registriescrates-ioprotocol)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<Value<RegistriesProtocol>>,
}

impl fmt::Debug for RegistriesConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { index, token, protocol } = self;
        let redacted_token = token
            .as_ref()
            .map(|token| Value { val: "[REDACTED]", definition: token.definition.clone() });
        f.debug_struct("RegistriesConfigValue")
            .field("index", &index)
            .field("token", &redacted_token)
            .field("protocol", &protocol)
            .finish()
    }
}

/// Specifies the protocol used to access crates.io.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registriescrates-ioprotocol)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum RegistriesProtocol {
    /// Causes Cargo to clone the entire index of all packages ever published to
    /// [crates.io](https://crates.io/) from <https://github.com/rust-lang/crates.io-index/>.
    Git,
    /// A newer protocol which uses HTTPS to download only what is necessary from
    /// <https://index.crates.io/>.
    Sparse,
}

impl FromStr for RegistriesProtocol {
    type Err = Error;

    fn from_str(protocol: &str) -> Result<Self, Self::Err> {
        match protocol {
            "git" => Ok(Self::Git),
            "sparse" => Ok(Self::Sparse),
            other => bail!("must be git or sparse, but found `{other}`"),
        }
    }
}

/// The `[registry]` table.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registry)
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct RegistryConfig {
    /// The name of the registry (from the
    /// [`registries` table](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registries))
    /// to use by default for registry commands like
    /// [`cargo publish`](https://doc.rust-lang.org/nightly/cargo/commands/cargo-publish.html).
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registrydefault)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value<String>>,
    /// Specifies the authentication token for [crates.io](https://crates.io/).
    ///
    /// Note: This library does not read any values in the
    /// [credentials](https://doc.rust-lang.org/nightly/cargo/reference/config.html#credentials)
    /// file.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#registrytoken)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<Value<String>>,
}

impl fmt::Debug for RegistryConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { default, token } = self;
        let redacted_token = token
            .as_ref()
            .map(|token| Value { val: "[REDACTED]", definition: token.definition.clone() });
        f.debug_struct("RegistryConfig")
            .field("default", &default)
            .field("token", &redacted_token)
            .finish()
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
    pub quiet: Option<Value<bool>>,
    /// Controls whether or not extra detailed messages are displayed by Cargo.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termverbose)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose: Option<Value<bool>>,
    /// Controls whether or not colored output is used in the terminal.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termcolor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Value<Color>>,
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
    pub when: Option<Value<When>>,
    /// Sets the width for progress bar.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswidth)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<Value<u32>>,
}

#[allow(clippy::exhaustive_enums)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Color {
    /// (default) Automatically detect if color support is available on the terminal.
    Auto,
    /// Always display colors.
    Always,
    /// Never display colors.
    Never,
}

impl Color {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::Auto
    }
}

impl FromStr for Color {
    type Err = Error;

    fn from_str(color: &str) -> Result<Self, Self::Err> {
        match color {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => bail!("must be auto, always, or never, but found `{other}`"),
        }
    }
}

#[allow(clippy::exhaustive_enums)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum When {
    /// (default) Intelligently guess whether to show progress bar.
    Auto,
    /// Always show progress bar.
    Always,
    /// Never show progress bar.
    Never,
}

impl When {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl Default for When {
    fn default() -> Self {
        Self::Auto
    }
}

impl FromStr for When {
    type Err = Error;

    fn from_str(color: &str) -> Result<Self, Self::Err> {
        match color {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => bail!("must be auto, always, or never, but found `{other}`"),
        }
    }
}

#[allow(clippy::exhaustive_enums)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Frequency {
    /// (default) Always display a notification when a command (e.g. `cargo build`)
    /// produces a future incompat report.
    Always,
    /// Never display a notification.
    Never,
}

impl Frequency {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl Default for Frequency {
    fn default() -> Self {
        Self::Always
    }
}

impl FromStr for Frequency {
    type Err = Error;

    fn from_str(color: &str) -> Result<Self, Self::Err> {
        match color {
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => bail!("must be always or never, but found `{other}`"),
        }
    }
}

/// A representation of rustflags and rustdocflags.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct Flags {
    pub flags: Vec<Value<String>>,
    // for merge
    #[serde(skip)]
    pub(crate) deserialized_repr: StringListDeserializedRepr,
}

impl Flags {
    /// Creates a rustflags from a string separated with ASCII unit separator ('\x1f').
    ///
    /// This is a valid format for the following environment variables:
    ///
    /// - `CARGO_ENCODED_RUSTFLAGS` (Cargo 1.55+)
    /// - `CARGO_ENCODED_RUSTDOCFLAGS` (Cargo 1.55+)
    ///
    /// See also `encode`.
    pub(crate) fn from_encoded(s: &Value<String>) -> Self {
        Self {
            flags: split_encoded(&s.val)
                .map(|v| Value { val: v.to_owned(), definition: s.definition.clone() })
                .collect(),
            // Encoded rustflags cannot be serialized as a string because they may contain spaces.
            deserialized_repr: StringListDeserializedRepr::Array,
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
    /// See also `encode_space_separated`.
    pub(crate) fn from_space_separated(s: &str, def: Option<&Definition>) -> Self {
        Self {
            flags: split_space_separated(s)
                .map(|v| Value { val: v.to_owned(), definition: def.cloned() })
                .collect(),
            deserialized_repr: StringListDeserializedRepr::String,
        }
    }

    pub(crate) fn from_array(flags: Vec<Value<String>>) -> Self {
        Self { flags, deserialized_repr: StringListDeserializedRepr::Array }
    }
}

impl<'de> Deserialize<'de> for Flags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: StringOrArray = Deserialize::deserialize(deserializer)?;
        match v {
            StringOrArray::String(s) => {
                Ok(Self::from_space_separated(&s.val, s.definition.as_ref()))
            }
            StringOrArray::Array(v) => Ok(Self::from_array(v)),
        }
    }
}

// https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/util/config/path.rs
#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(transparent)]
pub struct ConfigRelativePath(pub(crate) Value<String>);

impl ConfigRelativePath {
    /// Returns the underlying value.
    pub fn value(&self) -> &Value<String> {
        &self.0
    }

    /// Returns the raw underlying configuration value for this key.
    pub fn raw_value(&self) -> &str {
        &self.0.val
    }

    // /// Resolves this configuration-relative path to an absolute path.
    // ///
    // /// This will always return an absolute path where it's relative to the
    // /// location for configuration for this value.
    // pub(crate) fn resolve_path(&self, current_dir: &Path) -> Cow<'_, Path> {
    //     self.0.resolve_as_path(current_dir)
    // }

    /// Resolves this configuration-relative path to either an absolute path or
    /// something appropriate to execute from `PATH`.
    ///
    /// Values which don't look like a filesystem path (don't contain `/` or
    /// `\`) will be returned as-is, and everything else will fall through to an
    /// absolute path.
    pub(crate) fn resolve_program(&self, current_dir: &Path) -> Cow<'_, Path> {
        self.0.resolve_as_program_path(current_dir)
    }
}

/// An executable path with arguments.
///
/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#executable-paths-with-arguments)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PathAndArgs {
    pub path: ConfigRelativePath,
    pub args: Vec<Value<String>>,

    // for merge
    pub(crate) deserialized_repr: StringListDeserializedRepr,
}

impl PathAndArgs {
    pub(crate) fn from_string(value: &str, definition: Option<Definition>) -> Option<Self> {
        let mut s = split_space_separated(value);
        let path = s.next()?;
        Some(Self {
            args: s.map(|v| Value { val: v.to_owned(), definition: definition.clone() }).collect(),
            path: ConfigRelativePath(Value { val: path.to_owned(), definition }),
            deserialized_repr: StringListDeserializedRepr::String,
        })
    }
    pub(crate) fn from_array(mut list: Vec<Value<String>>) -> Option<Self> {
        if list.is_empty() {
            return None;
        }
        let path = list.remove(0);
        Some(Self {
            path: ConfigRelativePath(path),
            args: list,
            deserialized_repr: StringListDeserializedRepr::Array,
        })
    }
}

impl Serialize for PathAndArgs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.deserialized_repr {
            StringListDeserializedRepr::String => {
                let mut s = self.path.raw_value().to_owned();
                for arg in &self.args {
                    s.push(' ');
                    s.push_str(&arg.val);
                }
                s.serialize(serializer)
            }
            StringListDeserializedRepr::Array => {
                let mut v = Vec::with_capacity(1 + self.args.len());
                v.push(&self.path.0.val);
                for arg in &self.args {
                    v.push(&arg.val);
                }
                v.serialize(serializer)
            }
        }
    }
}
impl<'de> Deserialize<'de> for PathAndArgs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrArray {
            String(String),
            Array(Vec<Value<String>>),
        }
        let v: StringOrArray = Deserialize::deserialize(deserializer)?;
        let res = match v {
            StringOrArray::String(s) => Self::from_string(&s, None),
            StringOrArray::Array(v) => Self::from_array(v),
        };
        match res {
            Some(path) => Ok(path),
            None => Err(de::Error::invalid_length(0, &"at least one element")),
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StringList {
    pub list: Vec<Value<String>>,

    // for merge
    pub(crate) deserialized_repr: StringListDeserializedRepr,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum StringListDeserializedRepr {
    String,
    Array,
}
impl StringListDeserializedRepr {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Array => "array",
        }
    }
}

impl StringList {
    pub(crate) fn from_string(value: &str, definition: Option<&Definition>) -> Self {
        Self {
            list: split_space_separated(value)
                .map(|v| Value { val: v.to_owned(), definition: definition.cloned() })
                .collect(),
            deserialized_repr: StringListDeserializedRepr::String,
        }
    }
    pub(crate) fn from_array(list: Vec<Value<String>>) -> Self {
        Self { list, deserialized_repr: StringListDeserializedRepr::Array }
    }
}

impl Serialize for StringList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.deserialized_repr {
            StringListDeserializedRepr::String => {
                let mut s = String::with_capacity(
                    self.list.len().saturating_sub(1)
                        + self.list.iter().map(|v| v.val.len()).sum::<usize>(),
                );
                for arg in &self.list {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(&arg.val);
                }
                s.serialize(serializer)
            }
            StringListDeserializedRepr::Array => self.list.serialize(serializer),
        }
    }
}
impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: StringOrArray = Deserialize::deserialize(deserializer)?;
        match v {
            StringOrArray::String(s) => Ok(Self::from_string(&s.val, s.definition.as_ref())),
            StringOrArray::Array(v) => Ok(Self::from_array(v)),
        }
    }
}

/// A string or array of strings.
#[allow(clippy::exhaustive_enums)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray {
    String(Value<String>),
    Array(Vec<Value<String>>),
}

impl StringOrArray {
    pub(crate) const fn kind(&self) -> &'static str {
        match self {
            Self::String(..) => "string",
            Self::Array(..) => "array",
        }
    }

    // pub(crate) fn string(&self) -> Option<&Value<String>> {
    //     match self {
    //         Self::String(s) => Some(s),
    //         Self::Array(_) => None,
    //     }
    // }
    // pub(crate) fn array(&self) -> Option<&[Value<String>]> {
    //     match self {
    //         Self::String(_) => None,
    //         Self::Array(v) => Some(v),
    //     }
    // }
    pub(crate) fn as_array_no_split(&self) -> &[Value<String>] {
        match self {
            Self::String(s) => slice::from_ref(s),
            Self::Array(v) => v,
        }
    }
}

fn target_u_lower(target: &str) -> String {
    target.replace(['-', '.'], "_")
}
pub(crate) fn target_u_upper(target: &str) -> String {
    let mut target = target_u_lower(target);
    target.make_ascii_uppercase();
    target
}

pub(crate) fn split_encoded(s: &str) -> impl Iterator<Item = &str> {
    s.split('\x1f')
}
pub(crate) fn split_space_separated(s: &str) -> impl Iterator<Item = &str> {
    s.split(' ').map(str::trim).filter(|s| !s.is_empty())
}
