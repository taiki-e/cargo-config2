use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    path::Path,
};

use anyhow::Result;
use cfg_expr::{target_lexicon, Expression, Predicate};

use crate::{Config, Definition, Value};

#[allow(missing_debug_implementations)]
pub struct ResolveContext {
    pub(crate) env: HashMap<String, OsString>,
    rustc: Option<OsString>,
    cfg: HashMap<TargetTriple, Cfg>,
}

impl Default for ResolveContext {
    fn default() -> Self {
        Self::with_env(std::env::vars_os())
    }
}

impl ResolveContext {
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
        Self { env: HashMap::default(), rustc: None, cfg: HashMap::default() }
    }

    pub fn with_rustc(&mut self, rustc: impl AsRef<OsStr>) -> &mut Self {
        self.rustc = Some(rustc.as_ref().to_owned());
        self.cfg = HashMap::default();
        self
    }

    pub(crate) fn env(&self, name: &str) -> Result<Option<String>> {
        match self.env.get(name) {
            None => Ok(None),
            Some(v) => Ok(Some(v.clone().into_string().map_err(std::env::VarError::NotUnicode)?)),
        }
    }
    pub(crate) fn env_val(&self, name: &str) -> Result<Option<Value<String>>> {
        match self.env.get(name) {
            None => Ok(None),
            Some(v) => Ok(Some(Value {
                val: v.clone().into_string().map_err(std::env::VarError::NotUnicode)?,
                definition: Some(Definition::Environment(name.to_owned())),
            })),
        }
    }

    pub(crate) fn eval_cfg(&mut self, expr: &str, target: &TargetTriple) -> Result<bool> {
        let expr = Expression::parse(expr)?;
        let cfg = match self.cfg.get(target) {
            Some(cfg) => cfg,
            None => match self.cfg_for_target(target)? {
                Some(cfg) => {
                    self.cfg.insert(target.clone(), cfg);
                    &self.cfg[target]
                }
                None => return Ok(false),
            },
        };
        Ok(expr.eval(|pred| match pred {
            Predicate::Target(pred) => match &cfg.target_info {
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

    fn cfg_for_target(&mut self, target: &TargetTriple) -> Result<Option<Cfg>> {
        if let Some(rustc) = &self.rustc {
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
}

#[derive(Debug)]
struct Cfg {
    target_info: TargetInfo,
    target_features: HashSet<String>,
    flags: HashSet<String>,
    key_values: HashMap<String, HashSet<String>>,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum TargetInfo {
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

    fn from_rustc(rustc: &OsStr, target: &TargetTriple) -> Result<Self> {
        let list = cmd!(
            rustc,
            "--print",
            "cfg",
            "--target",
            target.spec_path.as_ref().unwrap_or(&target.triple)
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
            target_info: TargetInfo::CfgExpr(cfg_expr::targets::TargetInfo {
                triple: cfg_expr::targets::Triple::new_const(""), // we don't use this field
                os,
                abi,
                arch: arch.unwrap(),
                env,
                vendor,
                families: cfg_expr::targets::Families::new(families),
                pointer_width: pointer_width.unwrap(),
                endian: endian.unwrap(),
                has_atomics: cfg_expr::targets::HasAtomics::new(has_atomics),
                // TODO: don't unwrap -- cfg(panic) doesn't available on old stable rustc
                panic: panic.unwrap(),
            }),
            target_features,
            flags,
            key_values,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetTriple {
    pub(crate) triple: String,
    pub(crate) spec_path: Option<String>,
}

pub(crate) fn is_spec_path(triple_or_spec_path: &str) -> bool {
    Path::new(triple_or_spec_path).extension() == Some(OsStr::new("json"))
        || triple_or_spec_path.contains('/')
        || triple_or_spec_path.contains('\\')
}
fn resolve_spec_path(spec_path: &str, definition: Option<(&Definition, &Config)>) -> String {
    if let Some((definition, config)) = definition {
        if let Some(root) = definition.root(config) {
            return root.join(spec_path).into_os_string().into_string().unwrap();
        }
    }
    spec_path.to_owned()
}

impl TargetTriple {
    pub(crate) fn new(
        triple_or_spec_path: &str,
        definition: Option<(&Definition, &Config)>,
    ) -> Self {
        // Handles custom target
        if is_spec_path(triple_or_spec_path) {
            Self {
                triple: Path::new(triple_or_spec_path)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                spec_path: Some(resolve_spec_path(triple_or_spec_path, definition)),
            }
        } else {
            Self { triple: triple_or_spec_path.to_owned(), spec_path: None }
        }
    }

    pub fn triple(&self) -> &str {
        &self.triple
    }
    pub fn spec_path(&self) -> Option<&str> {
        self.spec_path.as_deref()
    }
}

impl From<&TargetTriple> for TargetTriple {
    fn from(value: &TargetTriple) -> Self {
        value.clone()
    }
}
impl From<String> for TargetTriple {
    fn from(value: String) -> Self {
        Self::new(&value, None)
    }
}
impl From<&String> for TargetTriple {
    fn from(value: &String) -> Self {
        Self::new(value, None)
    }
}
impl From<&str> for TargetTriple {
    fn from(value: &str) -> Self {
        Self::new(value, None)
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
}
