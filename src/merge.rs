// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::{btree_map, BTreeMap};

use crate::{
    de::{self, RegistriesProtocol},
    error::{Context as _, Result},
    value::Value,
    Color, Frequency, When,
};

// https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/util/config/mod.rs#L1900-L1908
//
// > If `force` is true, primitive (non-container) types will override existing values.
// > If false, the original will be kept and the new value ignored.
// >
// > Container types (tables and arrays) are merged with existing values.
// >
// > Container and non-container types cannot be mixed.
pub(crate) trait Merge {
    /// Merges given config into this config.
    fn merge(&mut self, from: Self, force: bool) -> Result<()>;
}

macro_rules! merge_non_container {
    ($($ty:tt)*) => {
        impl Merge for $($ty)* {
            fn merge(&mut self, from: Self, force: bool) -> Result<()> {
                if force {
                    *self = from;
                }
                Ok(())
            }
        }
        impl Merge for Value<$($ty)*> {
            fn merge(&mut self, from: Self, force: bool) -> Result<()> {
                if force {
                    *self = from;
                }
                Ok(())
            }
        }
    };
}
merge_non_container!(bool);
merge_non_container!(i32);
merge_non_container!(u32);
merge_non_container!(String);
merge_non_container!(Color);
merge_non_container!(Frequency);
merge_non_container!(When);
merge_non_container!(RegistriesProtocol);

impl<T: Merge> Merge for Option<T> {
    fn merge(&mut self, from: Self, force: bool) -> Result<()> {
        match (self, from) {
            (_, None) => {}
            (this @ None, from) => *this = from,
            (Some(this), Some(from)) => this.merge(from, force)?,
        }
        Ok(())
    }
}
impl Merge for de::StringOrArray {
    fn merge(&mut self, from: Self, force: bool) -> Result<()> {
        match (self, from) {
            (this @ de::StringOrArray::String(_), from @ de::StringOrArray::String(_)) => {
                if force {
                    *this = from;
                }
            }
            (de::StringOrArray::Array(this), de::StringOrArray::Array(mut from)) => {
                this.append(&mut from);
            }
            (expected, actual) => {
                bail!("expected {}, but found {}", expected.kind(), actual.kind());
            }
        }
        Ok(())
    }
}
impl Merge for de::PathAndArgs {
    fn merge(&mut self, mut from: Self, force: bool) -> Result<()> {
        match (self.deserialized_repr, from.deserialized_repr) {
            (de::StringListDeserializedRepr::String, de::StringListDeserializedRepr::String) => {
                if force {
                    *self = from;
                }
            }
            (de::StringListDeserializedRepr::Array, de::StringListDeserializedRepr::Array) => {
                // This is a bit non-intuitive, but e.g., "echo a <doc-path>/index.html"
                // is called in the following case because they are arrays.
                //
                // # a/b/.cargo/config
                // [doc]
                // browser = ["echo"]
                //
                // # a/.cargo/config
                // [doc]
                // browser = ["a"]
                self.args.push(from.path.0);
                self.args.append(&mut from.args);
            }
            (expected, actual) => {
                bail!("expected {}, but found {}", expected.as_str(), actual.as_str());
            }
        }
        Ok(())
    }
}
impl Merge for de::StringList {
    fn merge(&mut self, mut from: Self, force: bool) -> Result<()> {
        match (self.deserialized_repr, from.deserialized_repr) {
            (de::StringListDeserializedRepr::String, de::StringListDeserializedRepr::String) => {
                if force {
                    *self = from;
                }
            }
            (de::StringListDeserializedRepr::Array, de::StringListDeserializedRepr::Array) => {
                self.list.append(&mut from.list);
            }
            (expected, actual) => {
                bail!("expected {}, but found {}", expected.as_str(), actual.as_str());
            }
        }
        Ok(())
    }
}
impl Merge for de::EnvConfigValue {
    fn merge(&mut self, from: Self, force: bool) -> Result<()> {
        match (self, from) {
            (Self::Value(this), Self::Value(from)) => {
                if force {
                    *this = from;
                }
            }
            (
                Self::Table { value: this_value, force: this_force, relative: this_relative },
                Self::Table { value: from_value, force: from_force, relative: from_relative },
            ) => {
                this_value.merge(from_value, force)?;
                this_force.merge(from_force, force)?;
                this_relative.merge(from_relative, force)?;
            }
            (expected, actual) => {
                bail!("expected {}, but found {}", expected.kind(), actual.kind());
            }
        }
        Ok(())
    }
}
impl Merge for de::Flags {
    fn merge(&mut self, mut from: Self, force: bool) -> Result<()> {
        match (self.deserialized_repr, from.deserialized_repr) {
            (de::StringListDeserializedRepr::String, de::StringListDeserializedRepr::String) => {
                if force {
                    *self = from;
                }
            }
            (de::StringListDeserializedRepr::Array, de::StringListDeserializedRepr::Array) => {
                self.flags.append(&mut from.flags);
            }
            (expected, actual) => {
                bail!("expected {}, but found {}", expected.as_str(), actual.as_str());
            }
        }
        Ok(())
    }
}
impl<V: Merge + Clone + core::fmt::Debug> Merge for BTreeMap<String, V> {
    fn merge(&mut self, from: Self, force: bool) -> Result<()> {
        for (key, value) in from {
            match self.entry(key.clone()) {
                btree_map::Entry::Occupied(mut entry) => {
                    let entry = entry.get_mut();
                    entry.merge(value.clone(), force).with_context(|| {
                        format!(
                            "failed to merge key `{key}` between \
                             {entry:?} and {value:?}", /* TODO: do not use debug output */
                        )
                    })?;
                }
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(value);
                }
            };
        }
        Ok(())
    }
}
