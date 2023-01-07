use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    path::Path,
};

use anyhow::Result;
use cfg_expr::{target_lexicon, Expression, Predicate};
use serde::{Deserialize, Serialize};

use crate::{Definition, Value};

#[derive(Debug, Clone)]
#[must_use]
pub struct ResolveContext {
    pub(crate) env: HashMap<String, OsString>,
    rustc: Option<OsString>,
    cfg: RefCell<HashMap<TargetTriple, Cfg>>,
}

impl ResolveContext {
    pub fn new() -> Result<Self> {
        Ok(Self::with_env(std::env::vars_os()))
    }

    pub fn with_env(
        vars: impl IntoIterator<Item = (impl Into<OsString>, impl Into<OsString>)>,
    ) -> Self {
        let mut this = Self::no_env();
        for (k, v) in vars {
            if let Ok(k) = k.into().into_string() {
                if k.starts_with("CARGO_") || k.starts_with("RUST") || k == "BROWSER" {
                    this.env.insert(k, v.into());
                }
            }
        }
        this
    }

    pub fn no_env() -> Self {
        Self { env: HashMap::default(), rustc: None, cfg: RefCell::new(HashMap::default()) }
    }

    pub fn set_rustc(mut self, rustc: impl Into<OsString>) -> Self {
        self.rustc = Some(rustc.into());
        self.cfg = RefCell::new(HashMap::default());
        self
    }

    //  micro-optimization for static name -- avoiding name allocation can speed up
    // de::Config::apply_env by up to 40% because most env var names we fetch are static.
    pub(crate) fn env(&self, name: &'static str) -> Result<Option<Value<String>>> {
        match self.env.get(name) {
            None => Ok(None),
            Some(v) => Ok(Some(Value {
                val: v.clone().into_string().map_err(std::env::VarError::NotUnicode)?,
                definition: Some(Definition::Environment(name.into())),
            })),
        }
    }
    pub(crate) fn env_dyn(&self, name: &str) -> Result<Option<Value<String>>> {
        match self.env.get(name) {
            None => Ok(None),
            Some(v) => Ok(Some(Value {
                val: v.clone().into_string().map_err(std::env::VarError::NotUnicode)?,
                definition: Some(Definition::Environment(name.to_owned().into())),
            })),
        }
    }

    pub(crate) fn eval_cfg(&self, expr: &str, target: &TargetTripleRef<'_>) -> Result<bool> {
        let mut cfg_map = self.cfg.borrow_mut();
        let expr = Expression::parse(expr)?;
        let cfg = match cfg_map.get(target) {
            Some(cfg) => cfg,
            None => match cfg_for_target(self.rustc.as_deref(), target)? {
                Some(cfg) => {
                    cfg_map.insert(target.clone().into_owned(), cfg);
                    &cfg_map[target]
                }
                None => return Ok(false),
            },
        };
        Ok(expr.eval(|pred| match pred {
            Predicate::Target(pred) => match &cfg.target_info {
                TargetInfo::Cfg(target_info) => target_info.matches(pred),
                TargetInfo::CfgExpr(target_info) => pred.matches(target_info),
                TargetInfo::TargetLexicon(target_info) => pred.matches(target_info),
            },
            Predicate::TargetFeature(feature) => cfg.target_features.contains(*feature),
            Predicate::Flag(flag) => cfg.flags.contains(*flag),
            Predicate::KeyValue { key, val } => {
                cfg.key_values.get(*key).map_or(false, |values| values.contains(*val))
            }
            Predicate::Test
            | Predicate::DebugAssertions
            | Predicate::ProcMacro
            | Predicate::Feature(_) => false,
        }))
    }
}

fn cfg_for_target(rustc: Option<&OsStr>, target: &TargetTripleRef<'_>) -> Result<Option<Cfg>> {
    if let Some(rustc) = rustc {
        return Ok(Some(Cfg::from_rustc(rustc, target)?));
    }
    if let Some(target_info) = cfg_expr::targets::get_builtin_target_by_triple(&target.triple) {
        return Ok(Some(Cfg::from_target_info(TargetInfo::CfgExpr(target_info.clone()))));
    }
    // HACK: work around for https://github.com/bytecodealliance/target-lexicon/issues/63
    // Inspired by https://github.com/EmbarkStudios/cfg-expr/blob/0.13.0/tests/eval.rs#L19-L31.
    let triple = if target.triple.starts_with("avr-unknown-gnu-at") {
        target_lexicon::Triple {
            architecture: target_lexicon::Architecture::Avr,
            vendor: target_lexicon::Vendor::Unknown,
            operating_system: target_lexicon::OperatingSystem::Unknown,
            environment: target_lexicon::Environment::Unknown,
            binary_format: target_lexicon::BinaryFormat::Unknown,
        }
    } else {
        match target.triple.parse::<target_lexicon::Triple>() {
            Ok(triple) => triple,
            // TODO
            Err(_e) => return Ok(None),
        }
    };
    Ok(Some(Cfg::from_target_info(TargetInfo::TargetLexicon(triple))))
}

#[derive(Debug, Clone)]
struct Cfg {
    target_info: TargetInfo,
    target_features: HashSet<String>,
    flags: HashSet<String>,
    key_values: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum TargetInfo {
    Cfg(TargetCfg),
    CfgExpr(cfg_expr::targets::TargetInfo),
    TargetLexicon(target_lexicon::Triple),
}

impl Cfg {
    fn from_target_info(target_info: TargetInfo) -> Self {
        Self {
            target_info,
            target_features: HashSet::default(),
            flags: HashSet::default(),
            key_values: HashMap::default(),
        }
    }

    fn from_rustc(rustc: &OsStr, target: &TargetTripleRef<'_>) -> Result<Self> {
        let list = cmd!(
            rustc,
            "--print",
            "cfg",
            "--target",
            target.spec_path().unwrap_or(target.triple())
        )
        .read()?;
        Self::parse(&list)
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
                        // TODO: report error?
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
                            has_atomics.push(value.parse::<cfg_expr::targets::HasAtomic>()?);
                        }
                        "target_os" => os = Some(cfg_expr::targets::Os::new(value.to_owned())),
                        "target_pointer_width" => pointer_width = Some(value.parse::<u8>()?),
                        "target_vendor" => {
                            vendor = Some(cfg_expr::targets::Vendor::new(value.to_owned()));
                        }
                        // Unstable cfgs recognized by Cargo
                        "target_has_atomic_equal_alignment" | "target_has_atomic_load_store" => {
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
            target_info: TargetInfo::Cfg(TargetCfg {
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
            }),
            target_features,
            flags,
            key_values,
        })
    }
}

// Based on cfg_expr::targets::TargetInfo, but compatible with old rustc's cfg output.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetTripleRef<'a> {
    triple: Cow<'a, str>,
    spec_path: Option<Cow<'a, str>>,
}

pub type TargetTriple = TargetTripleRef<'static>;

pub(crate) fn is_spec_path(triple_or_spec_path: &str) -> bool {
    Path::new(triple_or_spec_path).extension() == Some(OsStr::new("json"))
        || triple_or_spec_path.contains('/')
        || triple_or_spec_path.contains('\\')
}
fn resolve_spec_path(
    spec_path: &str,
    def: Option<&Definition>,
    current_dir: Option<&Path>,
) -> Option<String> {
    if let Some(def) = def {
        if let Some(root) = def.root_opt(current_dir) {
            return Some(root.join(spec_path).into_os_string().to_string_lossy().into_owned());
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
                &Cow::Borrowed(v) => Path::new(v).file_stem().unwrap().to_string_lossy(),
                Cow::Owned(v) => {
                    Path::new(v).file_stem().unwrap().to_string_lossy().into_owned().into()
                }
            };
            Self {
                triple,
                spec_path: Some(match resolve_spec_path(&triple_or_spec_path, def, current_dir) {
                    Some(v) => Cow::Owned(v),
                    None => triple_or_spec_path,
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
    pub fn spec_path(&self) -> Option<&str> {
        self.spec_path.as_deref()
    }
}

impl<'a> From<&'a TargetTripleRef<'_>> for TargetTripleRef<'a> {
    fn from(value: &'a TargetTripleRef<'_>) -> Self {
        TargetTripleRef {
            triple: Cow::Borrowed(value.triple()),
            spec_path: value.spec_path().map(Into::into),
        }
    }
}
impl From<String> for TargetTripleRef<'static> {
    fn from(value: String) -> Self {
        Self::new(Cow::Owned(value), None, None)
    }
}
impl<'a> From<&'a String> for TargetTripleRef<'a> {
    fn from(value: &'a String) -> Self {
        Self::new(Cow::Borrowed(value), None, None)
    }
}
impl<'a> From<&'a str> for TargetTripleRef<'a> {
    fn from(value: &'a str) -> Self {
        Self::new(Cow::Borrowed(value), None, None)
    }
}

impl Serialize for TargetTripleRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.spec_path.as_ref().unwrap_or(&self.triple).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for TargetTripleRef<'_> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::new(String::deserialize(deserializer)?.into(), None, None))
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
    fn parse_cfg_list() {
        // builtins
        for target in duct::cmd!("rustc", "--print", "target-list").read().unwrap().lines() {
            let _cfg = Cfg::from_rustc(OsStr::new("rustc"), &target.into()).unwrap();
        }
        // custom targets
        for spec_path in fs::read_dir(fixtures_path().join("target-specs"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
        {
            let _cfg =
                Cfg::from_rustc(OsStr::new("rustc"), &spec_path.to_str().unwrap().into()).unwrap();
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
            ("CARGO_TERM_QUIET", "false"),
            ("CARGO_TERM_VERBOSE", "false"),
            ("CARGO_TERM_COLOR", "auto"),
            ("CARGO_TERM_PROGRESS_WHEN", "auto"),
            ("CARGO_TERM_PROGRESS_WIDTH", "100"),
        ];
        let mut config = crate::de::Config::default();
        let cx = &mut ResolveContext::with_env(env_list);
        for (k, v) in env_list {
            assert_eq!(cx.env[k], v, "key={k},value={v}");
        }
        config.apply_env(cx).unwrap();
    }

    // #[test]
    // fn dump_all_env() {
    //     let mut config = crate::de::Config::default();
    //     let cx = &mut ResolveContext::no_env();
    //     config.apply_env(cx).unwrap();
    // }
}
