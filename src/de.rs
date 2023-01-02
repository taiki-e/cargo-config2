use std::{collections::BTreeMap, num::NonZeroI32, path::PathBuf, slice, str::FromStr};

use anyhow::{bail, Error, Result};
use serde::{Deserialize, Serialize};

#[path = "de_env.rs"]
mod de_env;
#[path = "gen/de.rs"]
mod gen;

use crate::{value::Value, ResolveContext, TargetTriple};

/// Cargo configuration that environment variables, config overrides, and
/// target-specific configurations have not been resolved.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Config {
    // TODO: paths
    /// Command aliases.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#alias)
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub alias: BTreeMap<String, StringOrArray>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, Env>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub future_incompat_report: Option<FutureIncompatReportConfig>,
    // TODO: cargo-new
    // TODO: http
    // TODO: install
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<NetConfig>,
    // TODO: patch
    // TODO: profile
    // TODO: registries
    // TODO: registry
    // TODO: source
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub target: BTreeMap<String, TargetConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term: Option<TermConfig>,

    // Load contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    current_dir: Option<PathBuf>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Config {
    pub fn set_cwd(&mut self, path: PathBuf) {
        self.current_dir = Some(path);
    }

    pub fn set_path(&mut self, path: PathBuf) {
        crate::value::SetPath::set_path(self, &path);
        self.path = Some(path);
    }

    fn resolve_target(
        &self,
        cx: &mut ResolveContext,
        target_triple: &TargetTriple,
    ) -> Result<Option<TargetConfig>> {
        let target = &target_triple.triple;

        let target_u_upper = target_u_upper(target);
        let mut target_config = self.target.get(target).cloned();
        let mut target_linker = target_config.as_mut().and_then(|c| c.linker.take());
        let mut target_runner = target_config.as_mut().and_then(|c| c.runner.take());
        let mut target_rustflags: Option<Rustflags> =
            target_config.as_mut().and_then(|c| c.rustflags.take());
        if let Some(linker) = cx.env(&format!("CARGO_TARGET_{target_u_upper}_LINKER"))? {
            target_linker = Some(linker);
        }
        if let Some(runner) = cx.env(&format!("CARGO_TARGET_{target_u_upper}_RUNNER"))? {
            target_runner = Some(StringOrArray::String(runner));
        }
        if let Some(rustflags) = cx.env(&format!("CARGO_TARGET_{target_u_upper}_RUSTFLAGS"))? {
            let target_rustflags = target_rustflags.get_or_insert_with(Rustflags::default);
            let mut rustflags = Rustflags::from_space_separated(&rustflags);
            target_rustflags.flags.append(&mut rustflags.flags);
            if target_rustflags.deserialized_repr != rustflags.deserialized_repr {
                target_rustflags.deserialized_repr = RustflagsDeserializedRepr::Unknown;
            }
        }
        for (k, v) in &self.target {
            if !k.starts_with("cfg(") {
                continue;
            }
            if cx.eval_cfg(k, target_triple)? {
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
                    let target_rustflags = target_rustflags.get_or_insert_with(Rustflags::default);
                    target_rustflags.flags.extend_from_slice(&rustflags.flags);
                    if target_rustflags.deserialized_repr != rustflags.deserialized_repr {
                        target_rustflags.deserialized_repr = RustflagsDeserializedRepr::Unknown;
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
        if let Some(build) = &self.build {
            if build.override_target_rustflags {
                target_config.get_or_insert_with(TargetConfig::default).rustflags =
                    build.rustflags.clone();
            } else if let Some(rustflags) = target_rustflags {
                target_config.get_or_insert_with(TargetConfig::default).rustflags = Some(rustflags);
            } else {
                target_config.get_or_insert_with(TargetConfig::default).rustflags =
                    build.rustflags.clone();
            }
        } else if let Some(rustflags) = target_rustflags {
            target_config.get_or_insert_with(TargetConfig::default).rustflags = Some(rustflags);
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
    pub jobs: Option<Value<NonZeroI32>>,
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
    pub incremental: Option<Value<bool>>,
    /// Strips the given path prefix from dep info file paths.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#builddep-info-basedir)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dep_info_basedir: Option<Value<String>>,

    // Resolve contexts. Completely ignored in serialization and deserialization.
    #[serde(skip)]
    override_target_rustflags: bool,
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
    pub runner: Option<StringOrArray>,
    /// [reference (`target.<triple>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targettriplerustflags)
    ///
    /// [reference (`target.<cfg>.rustflags`)](https://doc.rust-lang.org/nightly/cargo/reference/config.html#targetcfgrustflags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustflags: Option<Rustflags>,
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
    pub browser: Option<StringOrArray>,
}

/// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#env)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Env {
    Value(Value<String>),
    Table {
        value: Value<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        force: Option<Value<bool>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        relative: Option<Value<bool>>,
    },
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
    pub color: Option<Value<When>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TermProgress>,
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
pub enum When {
    Auto,
    Always,
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
    Always,
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
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct Rustflags {
    pub(crate) flags: Vec<Value<String>>,
    // for merge
    #[serde(skip)]
    pub(crate) deserialized_repr: RustflagsDeserializedRepr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RustflagsDeserializedRepr {
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
    pub(crate) fn from_encoded(s: &Value<String>) -> Self {
        Self {
            flags: s
                .val
                .split('\x1f')
                .map(|v| Value { val: v.to_owned(), definition: s.definition.clone() })
                .collect(),
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
    pub(crate) fn from_space_separated(s: &Value<String>) -> Self {
        Self {
            flags: split_space_separated(&s.val)
                .map(|v| Value { val: v.to_owned(), definition: s.definition.clone() })
                .collect(),
            deserialized_repr: RustflagsDeserializedRepr::String,
        }
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

/// A string or array of strings.
#[allow(clippy::exhaustive_enums)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray {
    String(Value<String>),
    Array(Vec<Value<String>>),
}

impl StringOrArray {
    pub fn string(&self) -> Option<&Value<String>> {
        match self {
            Self::String(s) => Some(s),
            Self::Array(_) => None,
        }
    }
    pub fn array(&self) -> Option<&[Value<String>]> {
        match self {
            Self::String(_) => None,
            Self::Array(v) => Some(v),
        }
    }
    fn as_array_no_split(&self) -> &[Value<String>] {
        match self {
            Self::String(s) => slice::from_ref(s),
            Self::Array(v) => v,
        }
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
