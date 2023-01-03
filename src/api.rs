use std::{
    collections::{hash_map, HashMap},
    fmt, fs, mem,
    path::{Path, PathBuf},
};

use anyhow::{bail, format_err, Context as _, Result};
use once_cell::unsync::OnceCell;
use serde::Deserialize;
use toml_edit::easy as toml;

use crate::{de, easy, ConfigPaths, Definition, ResolveContext};

pub struct Config {
    cwd: PathBuf,
    cx: ResolveContext,

    values: OnceCell<HashMap<String, ConfigValue>>,
    build_config: OnceCell<easy::BuildConfig>,
}

impl Config {
    pub fn values(&self) -> Result<&HashMap<String, ConfigValue>> {
        self.values.get_or_try_init(|| self.load_values())
    }

    fn get<'de, T: Deserialize<'de> + Default>(&self, key: &str) -> Result<T> {
        let values = self.values()?;
        let value = match values.get(key) {
            Some(value) => value,
            None => return Ok(T::default()),
        };
        todo!()
    }

    pub fn build_config(&self) -> Result<&easy::BuildConfig> {
        self.build_config.get_or_try_init(|| self.get("build"))
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn load_values(&self) -> Result<HashMap<String, ConfigValue>> {
        // This definition path is ignored, this is just a temporary container
        // representing the entire file.
        let mut cfg = ConfigValue::Table(HashMap::new(), Definition::Path(PathBuf::from(".")));
        for path in ConfigPaths::new(&self.cwd) {
            let value = self.load_file(&path).context("could not load Cargo configuration")?;
            cfg.merge(value, false).with_context(|| {
                format!("failed to merge configuration at `{}`", path.display())
            })?;
        }
        match cfg {
            ConfigValue::Table(map, _) => Ok(map),
            _ => unreachable!(),
        }
    }

    fn load_file(&self, path: &Path) -> Result<ConfigValue> {
        let buf = fs::read_to_string(path)
            .with_context(|| format!("failed to read `{}`", path.display()))?;
        let value: toml::Value = toml::from_str(&buf)
            .with_context(|| format!("failed to parse `{}` as TOML", path.display()))?;
        ConfigValue::from_toml(Definition::Path(path.to_owned()), value)
    }
}

#[derive(Clone, PartialEq)]
pub enum ConfigValue {
    Integer(i64, Definition),
    String(String, Definition),
    List(Vec<(String, Definition)>, Definition),
    Table(HashMap<String, ConfigValue>, Definition),
    Boolean(bool, Definition),
}

impl fmt::Debug for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(i, def) => write!(f, "{i} (from {def})"),
            Self::Boolean(b, def) => write!(f, "{b} (from {def})"),
            Self::String(s, def) => write!(f, "{s} (from {def})"),
            Self::List(list, def) => {
                write!(f, "[")?;
                for (i, (s, def)) in list.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{s} (from {def})")?;
                }
                write!(f, "] (from {def})")
            }
            Self::Table(table, _) => write!(f, "{table:?}"),
        }
    }
}

impl ConfigValue {
    fn from_toml(def: Definition, toml: toml::Value) -> Result<ConfigValue> {
        match toml {
            toml::Value::String(val) => Ok(Self::String(val, def)),
            toml::Value::Boolean(b) => Ok(Self::Boolean(b, def)),
            toml::Value::Integer(i) => Ok(Self::Integer(i, def)),
            toml::Value::Array(val) => Ok(Self::List(
                val.into_iter()
                    .map(|toml| match toml {
                        toml::Value::String(val) => Ok((val, def.clone())),
                        v => bail!("expected string but found {} in list", v.type_str()),
                    })
                    .collect::<Result<_>>()?,
                def,
            )),
            toml::Value::Table(val) => Ok(Self::Table(
                val.into_iter()
                    .map(|(key, value)| {
                        let value = Self::from_toml(def.clone(), value)
                            .with_context(|| format!("failed to parse key `{key}`"))?;
                        Ok((key, value))
                    })
                    .collect::<Result<_>>()?,
                def,
            )),
            v => bail!("found TOML configuration value of unknown type `{}`", v.type_str()),
        }
    }

    fn into_toml(self) -> toml::Value {
        match self {
            Self::Boolean(s, _) => toml::Value::Boolean(s),
            Self::String(s, _) => toml::Value::String(s),
            Self::Integer(i, _) => toml::Value::Integer(i),
            Self::List(l, _) => {
                toml::Value::Array(l.into_iter().map(|(s, _)| toml::Value::String(s)).collect())
            }
            Self::Table(l, _) => {
                toml::Value::Table(l.into_iter().map(|(k, v)| (k, v.into_toml())).collect())
            }
        }
    }

    /// Merge the given value into self.
    ///
    /// If `force` is true, primitive (non-container) types will override existing values.
    /// If false, the original will be kept and the new value ignored.
    ///
    /// Container types (tables and arrays) are merged with existing values.
    ///
    /// Container and non-container types cannot be mixed.
    fn merge(&mut self, from: ConfigValue, force: bool) -> Result<()> {
        match (self, from) {
            (&mut Self::List(ref mut old, _), Self::List(ref mut new, _)) => {
                old.extend(mem::take(new).into_iter());
            }
            (&mut Self::Table(ref mut old, _), Self::Table(ref mut new, _)) => {
                for (key, value) in mem::take(new) {
                    match old.entry(key.clone()) {
                        hash_map::Entry::Occupied(mut entry) => {
                            let new_def = value.definition().clone();
                            let entry = entry.get_mut();
                            entry.merge(value, force).with_context(|| {
                                format!(
                                    "failed to merge key `{}` between \
                                     {} and {}",
                                    key,
                                    entry.definition(),
                                    new_def,
                                )
                            })?;
                        }
                        hash_map::Entry::Vacant(entry) => {
                            entry.insert(value);
                        }
                    };
                }
            }
            // Allow switching types except for tables or arrays.
            (expected @ &mut (Self::List(_, _) | Self::Table(_, _)), found)
            | (expected, found @ (Self::List(_, _) | Self::Table(_, _))) => {
                return Err(format_err!(
                    "failed to merge config value from `{}` into `{}`: expected {}, but found {}",
                    found.definition(),
                    expected.definition(),
                    expected.desc(),
                    found.desc()
                ));
            }
            (old, mut new) => {
                if force || new.definition().is_higher_priority(old.definition()) {
                    mem::swap(old, &mut new);
                }
            }
        }

        Ok(())
    }

    pub fn i64(&self, key: &str) -> Result<(i64, &Definition)> {
        match self {
            Self::Integer(i, def) => Ok((*i, def)),
            _ => self.expected("integer", key),
        }
    }

    pub fn string(&self, key: &str) -> Result<(&str, &Definition)> {
        match self {
            Self::String(s, def) => Ok((s, def)),
            _ => self.expected("string", key),
        }
    }

    pub fn table(&self, key: &str) -> Result<(&HashMap<String, ConfigValue>, &Definition)> {
        match self {
            Self::Table(table, def) => Ok((table, def)),
            _ => self.expected("table", key),
        }
    }

    pub fn list(&self, key: &str) -> Result<&[(String, Definition)]> {
        match self {
            Self::List(list, _) => Ok(list),
            _ => self.expected("list", key),
        }
    }

    pub fn boolean(&self, key: &str) -> Result<(bool, &Definition)> {
        match self {
            Self::Boolean(b, def) => Ok((*b, def)),
            _ => self.expected("bool", key),
        }
    }

    pub fn desc(&self) -> &'static str {
        match *self {
            Self::Table(..) => "table",
            Self::List(..) => "array",
            Self::String(..) => "string",
            Self::Boolean(..) => "boolean",
            Self::Integer(..) => "integer",
        }
    }

    pub fn definition(&self) -> &Definition {
        match self {
            Self::Boolean(_, def)
            | Self::Integer(_, def)
            | Self::String(_, def)
            | Self::List(_, def)
            | Self::Table(_, def) => def,
        }
    }

    fn expected<T>(&self, wanted: &str, key: &str) -> Result<T> {
        bail!(
            "expected a {}, but found a {} for `{}` in {}",
            wanted,
            self.desc(),
            key,
            self.definition()
        )
    }
}
