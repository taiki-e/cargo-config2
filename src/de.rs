use std::{
    collections::BTreeMap,
    num::NonZeroI32,
    ops,
    path::{Path, PathBuf},
    slice,
    str::FromStr,
};

use anyhow::{bail, Error, Result};
use serde::{Deserialize, Serialize};

#[path = "env_de.rs"]
mod env;

use crate::{
    value::{SetPath, Value},
    ResolveContext,
};

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
        self.target.set_path(&path);
        // self.term.set_path(&path);
        self.path = Some(path);
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

impl SetPath for BuildConfig {
    fn set_path(&mut self, path: &Path) {
        self.jobs.set_path(path);
        self.rustc.set_path(path);
        self.rustc_wrapper.set_path(path);
        self.rustc_workspace_wrapper.set_path(path);
        self.rustdoc.set_path(path);
        self.target.set_path(path);
        self.target_dir.set_path(path);
        // self.rustflags.set_path(&path);
        // self.rustdocflags.set_path(&path);
        self.incremental.set_path(&path);
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
}

impl SetPath for TargetConfig {
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
}

impl SetPath for DocConfig {
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
    pub(crate) deserialized_repr: EnvDeserializedRepr,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EnvDeserializedRepr {
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

impl SetPath for Env {
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
    pub when: Option<When>,
    /// Sets the width for progress bar.
    ///
    /// [reference](https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswidth)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
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
