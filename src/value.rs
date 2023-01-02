// Refs:
// - https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/util/config/value.rs

use std::{
    borrow::Cow,
    collections::BTreeMap,
    mem,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{de, split_space_separated, Config, StringOrArray};

#[allow(clippy::exhaustive_structs)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Value<T> {
    /// The inner value that was deserialized.
    pub val: T,
    /// The location where `val` was defined in configuration (e.g. file it was
    /// defined in, env var etc).
    #[serde(skip)]
    pub definition: Option<Definition>,
}

impl Value<String> {
    pub(crate) fn parse<T: FromStr>(self) -> Result<Value<T>, T::Err> {
        Ok(Value { val: self.val.parse()?, definition: self.definition })
    }
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#config-relative-paths
    pub(crate) fn resolve_as_program_path<'a>(
        &'a self,
        current_dir: Option<&Path>,
    ) -> Cow<'a, Path> {
        if self.definition.is_none()
            || Path::new(&self.val).is_absolute()
            || !self.val.contains('/') && !self.val.contains('\\')
        {
            Cow::Borrowed(Path::new(&self.val))
        } else if let Some(root) = self.definition.as_ref().unwrap().root_inner(current_dir) {
            root.join(&self.val).into()
        } else {
            Cow::Borrowed(Path::new(&self.val))
        }
    }
    pub(crate) fn resolve_as_path<'a>(&'a self, current_dir: Option<&Path>) -> Cow<'a, Path> {
        if self.definition.is_none() || Path::new(&self.val).is_absolute() {
            Cow::Borrowed(Path::new(&self.val))
        } else if let Some(root) = self.definition.as_ref().unwrap().root_inner(current_dir) {
            root.join(&self.val).into()
        } else {
            Cow::Borrowed(Path::new(&self.val))
        }
    }
}
impl StringOrArray<Value<String>> {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#executable-paths-with-arguments
    /// Splits this string or array of strings to program path with args.
    fn split_for_command(&self) -> Result<(&str, Option<&Definition>, Vec<&str>)> {
        match self {
            Self::String(s) => {
                let definition = s.definition.as_ref();
                let mut s = split_space_separated(&s.val);
                let path = s.next().context("invalid length 0, expected at least one element")?;
                Ok((path, definition, s.collect()))
            }
            Self::Array(v) => {
                let path = v.get(0).context("invalid length 0, expected at least one element")?;
                Ok((
                    &path.val,
                    path.definition.as_ref(),
                    v.iter().skip(1).map(|s| s.val.as_str()).collect(),
                ))
            }
        }
    }
    pub(crate) fn resolve_as_program_path_with_args<'a>(
        &'a self,
        current_dir: Option<&Path>,
    ) -> Result<(Cow<'a, Path>, Vec<&'a str>)> {
        let (program, definition, args) = self.split_for_command()?;
        if definition.is_none()
            || Path::new(program).is_absolute()
            || !program.contains('/') && !program.contains('\\')
        {
            Ok((Cow::Borrowed(Path::new(program)), args))
        } else if let Some(root) = definition.unwrap().root_inner(current_dir) {
            Ok((root.join(program).into(), args))
        } else {
            Ok((Cow::Borrowed(Path::new(program)), args))
        }
    }
}
impl de::StringOrArray {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#executable-paths-with-arguments
    /// Splits this string or array of strings to program path with args.
    fn split_for_command(&self) -> Result<(&str, Option<&Definition>, Vec<&str>)> {
        match self {
            Self::String(s) => {
                let definition = s.definition.as_ref();
                let mut s = split_space_separated(&s.val);
                let path = s.next().context("invalid length 0, expected at least one element")?;
                Ok((path, definition, s.collect()))
            }
            Self::Array(v) => {
                let path = v.get(0).context("invalid length 0, expected at least one element")?;
                Ok((
                    &path.val,
                    path.definition.as_ref(),
                    v.iter().skip(1).map(|s| s.val.as_str()).collect(),
                ))
            }
        }
    }
    pub(crate) fn resolve_as_program_path_with_args<'a>(
        &'a self,
        current_dir: Option<&Path>,
    ) -> Result<(Cow<'a, Path>, Vec<&'a str>)> {
        let (program, definition, args) = self.split_for_command()?;
        if definition.is_none()
            || Path::new(program).is_absolute()
            || !program.contains('/') && !program.contains('\\')
        {
            Ok((Cow::Borrowed(Path::new(program)), args))
        } else if let Some(root) = definition.unwrap().root_inner(current_dir) {
            Ok((root.join(program).into(), args))
        } else {
            Ok((Cow::Borrowed(Path::new(program)), args))
        }
    }
}

/// Location where a config value is defined.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Definition {
    /// Defined in a `.cargo/config`, includes the path to the file.
    Path(PathBuf),
    /// Defined in an environment variable, includes the environment key.
    Environment(String),
    /// Passed in on the command line.
    /// A path is attached when the config value is a path to a config file.
    Cli(Option<PathBuf>),
}

impl Definition {
    /// Root directory where this is defined.
    ///
    /// If from a file, it is the directory above `.cargo/config`.
    /// CLI and env are the current working directory.
    pub fn root<'a>(&'a self, config: &'a Config) -> Option<&'a Path> {
        self.root_inner(config.current_dir.as_deref())
    }
    pub(crate) fn root_inner<'a>(&'a self, current_dir: Option<&'a Path>) -> Option<&'a Path> {
        match self {
            Definition::Path(p) | Definition::Cli(Some(p)) => {
                Some(p.parent().unwrap().parent().unwrap())
            }
            Definition::Environment(_) | Definition::Cli(None) => current_dir,
        }
    }
}

// Refs: https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/util/config/value.rs#L91-L99
impl PartialEq for Definition {
    fn eq(&self, other: &Definition) -> bool {
        // configuration values are equivalent no matter where they're defined,
        // but they need to be defined in the same location. For example if
        // they're defined in the environment that's different than being
        // defined in a file due to path interpretations.
        mem::discriminant(self) == mem::discriminant(other)
    }
}

pub(crate) trait SetPath {
    fn set_path(&mut self, path: &Path);
}
impl<T: SetPath> SetPath for Option<T> {
    fn set_path(&mut self, path: &Path) {
        if let Some(v) = self {
            v.set_path(path);
        }
    }
}
impl<T: SetPath> SetPath for Vec<T> {
    fn set_path(&mut self, path: &Path) {
        for v in self {
            v.set_path(path);
        }
    }
}
impl<T: SetPath> SetPath for BTreeMap<String, T> {
    fn set_path(&mut self, path: &Path) {
        for v in self.values_mut() {
            v.set_path(path);
        }
    }
}
impl<T> SetPath for Value<T> {
    fn set_path(&mut self, path: &Path) {
        self.definition = Some(Definition::Path(path.to_owned()));
    }
}
impl<T> SetPath for StringOrArray<Value<T>> {
    fn set_path(&mut self, path: &Path) {
        match self {
            StringOrArray::String(s) => s.definition = Some(Definition::Path(path.to_owned())),
            StringOrArray::Array(v) => {
                for v in v {
                    v.definition = Some(Definition::Path(path.to_owned()));
                }
            }
        }
    }
}
