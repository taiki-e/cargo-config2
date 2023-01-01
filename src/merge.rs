// https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/util/config/mod.rs#L1900-L1908
//
// > If `force` is true, primitive (non-container) types will override existing values.
// > If false, the original will be kept and the new value ignored.
// >
// > Container types (tables and arrays) are merged with existing values.
// >
// > Container and non-container types cannot be mixed.

#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use std::collections::btree_map;

use anyhow::{Context as _, Result};

use crate::{
    BTreeMap, DeserializedRepr, Env, Frequency, NonZeroI32, Rustflags, StringOrArray, When,
};

pub(crate) trait Merge {
    /// Merges given config into this config.
    fn merge(&mut self, from: Self, force: bool) -> Result<()>;
}

macro_rules! merge_non_container {
    ($ty:tt) => {
        impl Merge for $ty {
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
merge_non_container!(u32);
merge_non_container!(NonZeroI32);
merge_non_container!(String);
merge_non_container!(Frequency);
merge_non_container!(When);

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
impl Merge for StringOrArray {
    fn merge(&mut self, from: Self, force: bool) -> Result<()> {
        match (self, from) {
            (this @ StringOrArray::String(_), from @ StringOrArray::String(_)) => {
                if force {
                    *this = from;
                }
            }
            (StringOrArray::Array(this), StringOrArray::Array(mut from)) => {
                this.append(&mut from);
            }
            _ => {
                todo!()
            }
        }
        Ok(())
    }
}
impl Merge for Env {
    fn merge(&mut self, from: Self, force: bool) -> Result<()> {
        match (self, from) {
            (this @ Env::Value(_), from @ Env::Value(_)) => {
                if force {
                    *this = from;
                }
            }
            (
                Env::Table { value: this_value, force: this_force, relative: this_relative },
                Env::Table { value: from_value, force: from_force, relative: from_relative },
            ) => {
                this_value.merge(from_value, force)?;
                this_force.merge(from_force, force)?;
                this_relative.merge(from_relative, force)?;
            }
            _ => {
                todo!()
            }
        }
        Ok(())
    }
}
impl Merge for Rustflags {
    fn merge(&mut self, mut from: Self, force: bool) -> Result<()> {
        match (self.deserialized_repr, from.deserialized_repr) {
            (DeserializedRepr::String, DeserializedRepr::String) => {
                if force {
                    *self = from;
                }
            }
            (DeserializedRepr::Array, DeserializedRepr::Array) => {
                self.flags.append(&mut from.flags);
            }
            (DeserializedRepr::Unknown, _) | (_, DeserializedRepr::Unknown) => {
                unreachable!()
            }
            _ => {
                todo!()
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
