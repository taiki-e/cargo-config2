use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::{paths::ConfigPaths, Config};

/// Reads cargo config file at the given path.
///
/// **Note:** This does not respect the hierarchical structure of the cargo config.
fn read(path: PathBuf) -> Result<Config> {
    let buf = fs::read(&path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut config: Config = toml_crate::from_slice(&buf)
        .with_context(|| format!("failed to parse `{}` as cargo configuration", path.display()))?;
    config.path = Some(path);
    Ok(config)
}

/// Hierarchically reads cargo config files and merge them.
pub(crate) fn read_hierarchical(current_dir: &Path) -> Result<Option<Config>> {
    let mut base = None;
    for path in ConfigPaths::new(current_dir) {
        let mut config = read(path.clone())?;
        config.current_dir = Some(current_dir.to_owned());
        match &mut base {
            None => base = Some(config),
            Some(base) => base.merge(config, false).with_context(|| {
                format!(
                    "failed to merge config from `{}` into `{}`",
                    path.display(),
                    base.path.as_ref().unwrap().display()
                )
            })?,
        }
    }
    Ok(base)
}

/// Hierarchically reads cargo config files.
pub(crate) fn read_hierarchical_unmerged(current_dir: &Path) -> Result<Vec<Config>> {
    let mut v = vec![];
    for path in ConfigPaths::new(current_dir) {
        let mut config = read(path)?;
        config.current_dir = Some(current_dir.to_owned());
        v.push(config);
    }
    Ok(v)
}
