use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    hash::Hash,
    path::{Path, PathBuf},
    str::FromStr,
};

use cfg_expr::{Expression, Predicate};
use once_cell::unsync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::{
    easy,
    error::{Context as _, Error, Result},
    process::ProcessBuilder,
    value::{Definition, Value},
    PathAndArgs,
};

#[derive(Debug, Clone, Default)]
#[must_use]
pub struct ResolveOptions {
    env: Option<HashMap<String, OsString>>,
    rustc: Option<PathAndArgs>,
    cargo: Option<OsString>,
    #[allow(clippy::option_option)]
    cargo_home: Option<Option<PathBuf>>,
    host_triple: Option<String>,
}

impl ResolveOptions {
    /// Sets `rustc` path and args.
    ///
    /// # Default value
    ///
    /// [`Config::rustc`](crate::Config::rustc)
    pub fn rustc(mut self, rustc: impl Into<PathAndArgs>) -> Self {
        self.rustc = Some(rustc.into());
        self
    }
    /// Sets `cargo` path.
    ///
    /// # Default value
    ///
    /// The value of the `CARGO` environment variable if it is set. Otherwise, "cargo".
    pub fn cargo(mut self, cargo: impl Into<OsString>) -> Self {
        self.cargo = Some(cargo.into());
        self
    }
    /// Sets `CARGO_HOME` path.
    ///
    /// # Default value
    ///
    /// [`home::cargo_home_with_cwd`] if the current directory was specified when
    /// loading config. Otherwise, [`home::cargo_home`].
    pub fn cargo_home(mut self, cargo_home: impl Into<Option<PathBuf>>) -> Self {
        self.cargo_home = Some(cargo_home.into());
        self
    }
    /// Sets host target triple.
    ///
    /// # Default value
    ///
    /// Parse the version output of `cargo` specified by [`Self::cargo`].
    pub fn host_triple(mut self, triple: impl Into<String>) -> Self {
        self.host_triple = Some(triple.into());
        self
    }
    /// Sets the specified key-values as environment variables to be read during
    /// config resolution.
    ///
    /// This is mainly intended for use in tests where it is necessary to adjust
    /// the kinds of environment variables that are referenced.
    ///
    /// # Default value
    ///
    /// [`std::env::vars_os`]
    pub fn env(
        mut self,
        vars: impl IntoIterator<Item = (impl Into<OsString>, impl Into<OsString>)>,
    ) -> Self {
        let mut env = HashMap::default();
        for (k, v) in vars {
            if let Ok(k) = k.into().into_string() {
                if k.starts_with("CARGO") || k.starts_with("RUST") || k == "BROWSER" {
                    env.insert(k, v.into());
                }
            }
        }
        self.env = Some(env);
        self
    }

    #[doc(hidden)] // Not public API.
    pub fn into_context(mut self) -> ResolveContext {
        if self.env.is_none() {
            self = self.env(std::env::vars_os());
        }
        let env = self.env.unwrap();
        let rustc = match self.rustc {
            Some(rustc) => OnceCell::from(rustc),
            None => OnceCell::new(),
        };
        let cargo = match self.cargo {
            Some(cargo) => cargo,
            None => env.get("CARGO").cloned().unwrap_or_else(|| "cargo".into()),
        };
        let cargo_home = match self.cargo_home {
            Some(cargo_home) => OnceCell::from(cargo_home),
            None => OnceCell::new(),
        };
        let host_triple = match self.host_triple {
            Some(host_triple) => OnceCell::from(host_triple),
            None => OnceCell::new(),
        };

        ResolveContext { env, rustc, cargo, cargo_home, host_triple, cfg: RefCell::default() }
    }
}

#[doc(hidden)] // Not public API.
#[derive(Debug, Clone)]
#[must_use]
pub struct ResolveContext {
    pub(crate) env: HashMap<String, OsString>,
    rustc: OnceCell<easy::PathAndArgs>,
    pub(crate) cargo: OsString,
    cargo_home: OnceCell<Option<PathBuf>>,
    host_triple: OnceCell<String>,
    cfg: RefCell<HashMap<TargetTripleBorrow<'static>, Cfg>>,
}

impl ResolveContext {
    pub(crate) fn rustc(&self, build_config: &easy::BuildConfig) -> &PathAndArgs {
        self.rustc.get_or_init(|| {
            // TODO: Update comment based on https://github.com/rust-lang/cargo/pull/10896?
            // The following priorities are not documented, but at as of cargo
            // 1.63.0-nightly (2022-05-31), `RUSTC_WRAPPER` is preferred over `RUSTC_WORKSPACE_WRAPPER`.
            // See also https://github.com/taiki-e/cargo-llvm-cov/pull/180#discussion_r887904341.
            let rustc =
                build_config.rustc.as_ref().map_or_else(|| rustc_path(&self.cargo), PathBuf::from);
            match build_config
                .rustc_wrapper
                .as_ref()
                .or(build_config.rustc_workspace_wrapper.as_ref())
            {
                // The wrapper's first argument is supposed to be the path to rustc.
                Some(wrapper) => {
                    PathAndArgs { path: wrapper.clone(), args: vec![rustc.into_os_string()] }
                }
                None => PathAndArgs { path: rustc, args: vec![] },
            }
        })
    }
    pub(crate) fn cargo_home(&self, cwd: &Path) -> &Option<PathBuf> {
        self.cargo_home.get_or_init(|| home::cargo_home_with_cwd(cwd).ok())
    }
    pub(crate) fn host_triple(&self) -> Result<&str> {
        Ok(self.host_triple.get_or_try_init(|| host_triple(&self.cargo))?)
    }

    //  micro-optimization for static name -- avoiding name allocation can speed up
    // de::Config::apply_env by up to 40% because most env var names we fetch are static.
    pub(crate) fn env(&self, name: &'static str) -> Result<Option<Value<String>>> {
        match self.env.get(name) {
            None => Ok(None),
            Some(v) => Ok(Some(Value {
                val: v.clone().into_string().map_err(|var| Error::env_not_unicode(name, var))?,
                definition: Some(Definition::Environment(name.into())),
            })),
        }
    }
    pub(crate) fn env_redacted(&self, name: &'static str) -> Result<Option<Value<String>>> {
        match self.env.get(name) {
            None => Ok(None),
            Some(v) => Ok(Some(Value {
                val: v
                    .clone()
                    .into_string()
                    .map_err(|_var| Error::env_not_unicode_redacted(name))?,
                definition: Some(Definition::Environment(name.into())),
            })),
        }
    }
    pub(crate) fn env_parse<T>(&self, name: &'static str) -> Result<Option<Value<T>>>
    where
        T: FromStr,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        match self.env(name)? {
            Some(v) => Ok(Some(
                v.parse()
                    .with_context(|| format!("failed to parse environment variable `{name}`"))?,
            )),
            None => Ok(None),
        }
    }
    pub(crate) fn env_dyn(&self, name: &str) -> Result<Option<Value<String>>> {
        match self.env.get(name) {
            None => Ok(None),
            Some(v) => Ok(Some(Value {
                val: v.clone().into_string().map_err(|var| Error::env_not_unicode(name, var))?,
                definition: Some(Definition::Environment(name.to_owned().into())),
            })),
        }
    }

    pub(crate) fn eval_cfg(
        &self,
        expr: &str,
        target: &TargetTripleRef<'_>,
        build_config: &easy::BuildConfig,
    ) -> Result<bool> {
        let mut cfg_map = self.cfg.borrow_mut();
        let expr = Expression::parse(expr).map_err(Error::new)?;
        let cfg = match cfg_map.get(target.cli_target()) {
            Some(cfg) => cfg,
            None => {
                let cfg = Cfg::from_rustc(self.rustc(build_config).clone().into(), target)?;
                cfg_map.insert(TargetTripleBorrow(target.clone().into_owned()), cfg);
                &cfg_map[target.cli_target()]
            }
        };
        Ok(expr.eval(|pred| match pred {
            Predicate::Target(pred) => cfg.target_info.matches(pred),
            Predicate::TargetFeature(feature) => cfg.target_features.contains(*feature),
            Predicate::Flag(flag) => cfg.flags.contains(*flag),
            Predicate::KeyValue { key, val } => {
                cfg.key_values.get(*key).map_or(false, |values| values.contains(*val))
            }
            // https://github.com/rust-lang/cargo/pull/7660
            Predicate::Test
            | Predicate::DebugAssertions
            | Predicate::ProcMacro
            | Predicate::Feature(_) => false,
        }))
    }
}

#[derive(Debug, Clone)]
struct Cfg {
    target_info: TargetCfg,
    target_features: HashSet<String>,
    flags: HashSet<String>,
    key_values: HashMap<String, HashSet<String>>,
}

impl Cfg {
    fn from_rustc(mut rustc: ProcessBuilder, target: &TargetTripleRef<'_>) -> Result<Self> {
        let cmd = rustc.args(["--print", "cfg", "--target", &*target.cli_target_string()]);
        let list = cmd.read()?;
        Self::parse(&list)
            .with_context(|| format_err!("unexpected cfg output from `{cmd}`: {list}"))
    }

    fn parse(list: &str) -> Result<Self> {
        let mut os = None;
        let mut abi = None;
        let mut arch = None;
        let mut env = None;
        let mut vendor = None;
        let mut families = vec![];
        let mut endian = None;
        let mut has_atomics = vec![];
        let mut panic = None;
        let mut pointer_width = None;
        let mut target_features = HashSet::default();
        let mut flags = HashSet::default();
        let mut key_values = HashMap::<String, HashSet<String>>::default();

        for line in list.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match line.split_once('=') {
                None => {
                    flags.insert(line.to_owned());
                }
                Some((name, value)) => {
                    if !value.starts_with('"') || !value.ends_with('"') {
                        #[cfg(test)]
                        panic!("invalid value '{value}'");
                        #[cfg(not(test))]
                        continue;
                    }
                    let value = &value[1..value.len() - 1];
                    if value.is_empty() {
                        continue;
                    }
                    match name {
                        "panic" => panic = Some(cfg_expr::targets::Panic::new(value.to_owned())),
                        "target_abi" => abi = Some(cfg_expr::targets::Abi::new(value.to_owned())),
                        "target_arch" => {
                            arch = Some(cfg_expr::targets::Arch::new(value.to_owned()));
                        }
                        "target_endian" => {
                            endian = Some(if value == "little" {
                                cfg_expr::targets::Endian::little
                            } else {
                                cfg_expr::targets::Endian::big
                            });
                        }
                        "target_env" => env = Some(cfg_expr::targets::Env::new(value.to_owned())),
                        "target_family" => {
                            families.push(cfg_expr::targets::Family::new(value.to_owned()));
                        }
                        "target_feature" => {
                            target_features.insert(value.to_owned());
                        }
                        "target_has_atomic" => {
                            has_atomics.push(
                                value
                                    .parse::<cfg_expr::targets::HasAtomic>()
                                    .context("failed to parse `target_has_atomic=\"..\"`")?,
                            );
                        }
                        "target_os" => os = Some(cfg_expr::targets::Os::new(value.to_owned())),
                        "target_pointer_width" => {
                            pointer_width = Some(
                                value
                                    .parse::<u8>()
                                    .context("failed to parse `target_pointer_width=\"..\"`")?,
                            );
                        }
                        "target_vendor" => {
                            vendor = Some(cfg_expr::targets::Vendor::new(value.to_owned()));
                        }
                        // Unstable cfgs recognized by Cargo
                        "target_has_atomic_equal_alignment"
                        | "target_has_atomic_load_store"
                        | "relocation_model" => {
                            if let Some(values) = key_values.get_mut(name) {
                                values.insert(value.to_owned());
                            } else {
                                let mut values = HashSet::default();
                                values.insert(value.to_owned());
                                key_values.insert(name.to_owned(), values);
                            }
                        }
                        #[cfg(test)]
                        _ => panic!("unrecognized cfg '{name}'"),
                        #[cfg(not(test))]
                        _ => {}
                    }
                }
            }
        }

        Ok(Cfg {
            target_info: TargetCfg {
                os,
                abi,
                arch: arch.unwrap(),
                env,
                vendor,
                families: cfg_expr::targets::Families::new(families),
                pointer_width: pointer_width.unwrap(),
                endian: endian.unwrap(),
                has_atomics: cfg_expr::targets::HasAtomics::new(has_atomics),
                panic,
            },
            target_features,
            flags,
            key_values,
        })
    }
}

// Based on cfg_expr::targets::TargetInfo, but compatible with old rustc's cfg output.
#[derive(Debug, Clone)]
struct TargetCfg {
    os: Option<cfg_expr::targets::Os>,
    abi: Option<cfg_expr::targets::Abi>,
    arch: cfg_expr::targets::Arch,
    env: Option<cfg_expr::targets::Env>,
    vendor: Option<cfg_expr::targets::Vendor>,
    families: cfg_expr::targets::Families,
    pointer_width: u8,
    endian: cfg_expr::targets::Endian,
    // available on stable 1.60+ or nightly
    has_atomics: cfg_expr::targets::HasAtomics,
    // available on stable 1.60+ or nightly
    panic: Option<cfg_expr::targets::Panic>,
}

impl TargetCfg {
    fn matches(&self, tp: &cfg_expr::TargetPredicate) -> bool {
        use cfg_expr::TargetPredicate::{
            Abi, Arch, Endian, Env, Family, HasAtomic, Os, Panic, PointerWidth, Vendor,
        };

        match tp {
            // The ABI is allowed to be an empty string
            Abi(abi) => match &self.abi {
                Some(a) => abi == a,
                None => abi.0.is_empty(),
            },
            Arch(a) => a == &self.arch,
            Endian(end) => *end == self.endian,
            // The environment is allowed to be an empty string
            Env(env) => match &self.env {
                Some(e) => env == e,
                None => env.0.is_empty(),
            },
            Family(fam) => self.families.contains(fam),
            HasAtomic(has_atomic) => self.has_atomics.contains(*has_atomic),
            Os(os) => Some(os) == self.os.as_ref(),
            PointerWidth(w) => *w == self.pointer_width,
            Vendor(ven) => match &self.vendor {
                Some(v) => ven == v,
                None => ven == &cfg_expr::targets::Vendor::unknown,
            },
            Panic(panic) => Some(panic) == self.panic.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TargetTripleRef<'a> {
    triple: Cow<'a, str>,
    spec_path: Option<Cow<'a, Path>>,
}

pub type TargetTriple = TargetTripleRef<'static>;

impl PartialEq for TargetTripleRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cli_target() == other.cli_target()
    }
}
impl Eq for TargetTripleRef<'_> {}
impl PartialOrd for TargetTripleRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TargetTripleRef<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cli_target().cmp(other.cli_target())
    }
}
impl Hash for TargetTripleRef<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cli_target().hash(state);
    }
}

// This wrapper is needed to support pre-1.63 Rust.
// In pre-1.63 Rust you can't use TargetTripleRef<'non_static> as an index of
// HashMap<TargetTripleRef<'static>, _> without this trick.
#[allow(single_use_lifetimes)] // false positive fixed in Rust 1.66.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct TargetTripleBorrow<'a>(pub(crate) TargetTripleRef<'a>);
impl std::borrow::Borrow<OsStr> for TargetTripleBorrow<'_> {
    fn borrow(&self) -> &OsStr {
        self.0.cli_target()
    }
}

fn is_spec_path(triple_or_spec_path: &str) -> bool {
    Path::new(triple_or_spec_path).extension() == Some(OsStr::new("json"))
        || triple_or_spec_path.contains('/')
        || triple_or_spec_path.contains('\\')
}
fn resolve_spec_path(
    spec_path: &str,
    def: Option<&Definition>,
    current_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(def) = def {
        if let Some(root) = def.root_opt(current_dir) {
            return Some(root.join(spec_path));
        }
    }
    None
}

impl<'a> TargetTripleRef<'a> {
    pub(crate) fn new(
        triple_or_spec_path: Cow<'a, str>,
        def: Option<&Definition>,
        current_dir: Option<&Path>,
    ) -> Self {
        // Handles custom target
        if is_spec_path(&triple_or_spec_path) {
            let triple = match &triple_or_spec_path {
                // `triple_or_spec_path` is valid UTF-8, so unwrap here will never panic.
                &Cow::Borrowed(v) => Path::new(v).file_stem().unwrap().to_str().unwrap().into(),
                Cow::Owned(v) => {
                    Path::new(v).file_stem().unwrap().to_str().unwrap().to_owned().into()
                }
            };
            Self {
                triple,
                spec_path: Some(match resolve_spec_path(&triple_or_spec_path, def, current_dir) {
                    Some(v) => v.into(),
                    None => match triple_or_spec_path {
                        Cow::Borrowed(v) => Path::new(v).into(),
                        Cow::Owned(v) => PathBuf::from(v).into(),
                    },
                }),
            }
        } else {
            Self { triple: triple_or_spec_path, spec_path: None }
        }
    }

    pub fn into_owned(self) -> TargetTriple {
        TargetTripleRef {
            triple: self.triple.into_owned().into(),
            spec_path: self.spec_path.map(|v| v.into_owned().into()),
        }
    }

    pub fn triple(&self) -> &str {
        &self.triple
    }
    pub fn spec_path(&self) -> Option<&Path> {
        self.spec_path.as_deref()
    }
    pub(crate) fn cli_target_string(&self) -> Cow<'_, str> {
        // Cargo converts spec path containing non-UTF8 byte to string with
        // to_string_lossy before passing it to rustc.
        // This is not good behavior but we just follow the behavior of cargo for now.
        //
        // ```
        // $ pwd
        // /tmp/��/a
        // $ cat .cargo/config.toml
        // [build]
        // target = "avr-unknown-gnu-atmega2560.json"
        // ```
        // $ cargo build
        // error: target path "/tmp/��/a/avr-unknown-gnu-atmega2560.json" is not a valid file
        //
        // Caused by:
        //   No such file or directory (os error 2)
        // ```
        self.cli_target().to_string_lossy()
    }
    pub(crate) fn cli_target(&self) -> &OsStr {
        match self.spec_path() {
            Some(v) => v.as_os_str(),
            None => OsStr::new(self.triple()),
        }
    }
}

impl<'a> From<&'a TargetTripleRef<'_>> for TargetTripleRef<'a> {
    fn from(value: &'a TargetTripleRef<'_>) -> Self {
        TargetTripleRef {
            triple: value.triple().into(),
            spec_path: value.spec_path().map(Into::into),
        }
    }
}
impl From<String> for TargetTripleRef<'static> {
    fn from(value: String) -> Self {
        Self::new(value.into(), None, None)
    }
}
impl<'a> From<&'a String> for TargetTripleRef<'a> {
    fn from(value: &'a String) -> Self {
        Self::new(value.into(), None, None)
    }
}
impl<'a> From<&'a str> for TargetTripleRef<'a> {
    fn from(value: &'a str) -> Self {
        Self::new(value.into(), None, None)
    }
}

impl Serialize for TargetTripleRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.cli_target_string().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for TargetTripleRef<'static> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::new(String::deserialize(deserializer)?.into(), None, None))
    }
}

/// Gets host triple of the given `rustc` or `cargo`.
pub(crate) fn host_triple(rustc_or_cargo: &OsStr) -> Result<String> {
    let mut cmd = cmd!(rustc_or_cargo, "--version", "--verbose");
    let verbose_version = cmd.read()?;
    let host = verbose_version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| format_err!("unexpected version output from `{cmd}`: {verbose_version}"))?
        .to_owned();
    Ok(host)
}

pub(crate) fn rustc_path(cargo: &OsStr) -> PathBuf {
    // When toolchain override shorthand (`+toolchain`) is used, `rustc` in
    // PATH and `CARGO` environment variable may be different toolchains.
    // When Rust was installed using rustup, the same toolchain's rustc
    // binary is in the same directory as the cargo binary, so we use it.
    let mut rustc = PathBuf::from(cargo);
    rustc.pop(); // cargo
    rustc.push(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    if rustc.exists() {
        rustc
    } else {
        "rustc".into()
    }
}

#[cfg(test)]
mod tests {
    use fs_err as fs;

    use super::*;

    fn fixtures_path() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
    }

    #[test]
    fn target_triple() {
        let t = TargetTripleRef::from("x86_64-unknown-linux-gnu");
        assert_eq!(t.triple, "x86_64-unknown-linux-gnu");
        assert!(matches!(t.triple, Cow::Borrowed(..)));
        assert!(t.spec_path.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Miri doesn't support pipe2 (inside duct::Expression::read)
    fn parse_cfg_list() {
        // builtin targets
        for target in duct::cmd!("rustc", "--print", "target-list").read().unwrap().lines() {
            let _cfg = Cfg::from_rustc(cmd!("rustc"), &target.into()).unwrap();
        }
        // custom targets
        for spec_path in fs::read_dir(fixtures_path().join("target-specs"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
        {
            let _cfg = Cfg::from_rustc(cmd!("rustc"), &spec_path.to_str().unwrap().into()).unwrap();
        }
    }

    #[test]
    fn env_filter() {
        // NB: sync with bench in bench/benches/bench.rs
        let env_list = [
            ("CARGO_BUILD_JOBS", "-1"),
            ("RUSTC", "rustc"),
            ("CARGO_BUILD_RUSTC", "rustc"),
            ("RUSTC_WRAPPER", "rustc_wrapper"),
            ("CARGO_BUILD_RUSTC_WRAPPER", "rustc_wrapper"),
            ("RUSTC_WORKSPACE_WRAPPER", "rustc_workspace_wrapper"),
            ("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "rustc_workspace_wrapper"),
            ("RUSTDOC", "rustdoc"),
            ("CARGO_BUILD_RUSTDOC", "rustdoc"),
            ("CARGO_BUILD_TARGET", "triple"),
            ("CARGO_TARGET_DIR", "target"),
            ("CARGO_BUILD_TARGET_DIR", "target"),
            ("CARGO_ENCODED_RUSTFLAGS", "1"),
            ("RUSTFLAGS", "1"),
            ("CARGO_BUILD_RUSTFLAGS", "1"),
            ("CARGO_ENCODED_RUSTDOCFLAGS", "1"),
            ("RUSTDOCFLAGS", "1"),
            ("CARGO_BUILD_RUSTDOCFLAGS", "1"),
            ("CARGO_INCREMENTAL", "false"),
            ("CARGO_BUILD_INCREMENTAL", "1"),
            ("CARGO_BUILD_DEP_INFO_BASEDIR", "1"),
            ("BROWSER", "1"),
            ("CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY", "always"),
            ("CARGO_NET_RETRY", "1"),
            ("CARGO_NET_GIT_FETCH_WITH_CLI", "false"),
            ("CARGO_NET_OFFLINE", "false"),
            ("CARGO_REGISTRIES_crates-io_INDEX", "https://github.com/rust-lang/crates.io-index"),
            ("CARGO_REGISTRIES_crates-io_TOKEN", "00000000000000000000000000000000000"),
            ("CARGO_REGISTRY_DEFAULT", "crates-io"),
            ("CARGO_REGISTRY_TOKEN", "00000000000000000000000000000000000"),
            ("CARGO_REGISTRIES_CRATES_IO_PROTOCOL", "git"),
            ("CARGO_TERM_QUIET", "false"),
            ("CARGO_TERM_VERBOSE", "false"),
            ("CARGO_TERM_COLOR", "auto"),
            ("CARGO_TERM_PROGRESS_WHEN", "auto"),
            ("CARGO_TERM_PROGRESS_WIDTH", "100"),
        ];
        let mut config = crate::de::Config::default();
        let cx = &ResolveOptions::default().env(env_list).into_context();
        config.apply_env(cx).unwrap();

        // ResolveOptions::env attempts to avoid pushing unrelated envs.
        let mut env_list = env_list.to_vec();
        env_list.push(("A", "B"));
        let cx = &ResolveOptions::default().env(env_list.iter().copied()).into_context();
        for (k, v) in env_list {
            if k == "A" {
                assert!(!cx.env.contains_key(k));
            } else {
                assert_eq!(cx.env[k], v, "key={k},value={v}");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn env_no_utf8() {
        use std::{ffi::OsStr, os::unix::prelude::OsStrExt};

        let cx = &ResolveOptions::default()
            .env([("CARGO_ALIAS_a", OsStr::from_bytes(&[b'f', b'o', 0x80, b'o']))])
            .cargo_home(None)
            .rustc(PathAndArgs::new("rustc"))
            .into_context();
        assert_eq!(
            cx.env("CARGO_ALIAS_a").unwrap_err().to_string(),
            "failed to parse environment variable `CARGO_ALIAS_a`"
        );
        assert_eq!(
            format!("{:#}", anyhow::Error::from(cx.env("CARGO_ALIAS_a").unwrap_err())),
            "failed to parse environment variable `CARGO_ALIAS_a`: environment variable was not valid unicode: \"fo\\x80o\""
        );
    }

    // #[test]
    // fn dump_all_env() {
    //     let mut config = crate::de::Config::default();
    //     let cx = &mut ResolveContext::no_env();
    //     config.apply_env(cx).unwrap();
    // }
}
