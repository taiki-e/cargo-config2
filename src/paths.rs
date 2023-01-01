// https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure
//
// > Cargo allows local configuration for a particular package as well as global
// > configuration. It looks for configuration files in the current directory
// > and all parent directories. If, for example, Cargo were invoked in
// > `/projects/foo/bar/baz`, then the following configuration files would be
// > probed for and unified in this order:
// >
// > - `/projects/foo/bar/baz/.cargo/config.toml`
// > - `/projects/foo/bar/.cargo/config.toml`
// > - `/projects/foo/.cargo/config.toml`
// > - `/projects/.cargo/config.toml`
// > - `/.cargo/config.toml`
// > - `$CARGO_HOME/config.toml` which defaults to:
// >   - Windows: `%USERPROFILE%\.cargo\config.toml`
// >   - Unix: `$HOME/.cargo/config.toml`

use std::path::{Path, PathBuf};

fn config_path(path: &Path) -> Option<PathBuf> {
    // https://doc.rust-lang.org/nightly/cargo/reference/config.html#hierarchical-structure
    //
    // > Cargo also reads config files without the `.toml` extension,
    // > such as `.cargo/config`. Support for the `.toml` extension was
    // > added in version 1.39 and is the preferred form. If both files
    // > exist, Cargo will use the file without the extension.
    let config = path.join("config");
    if config.exists() {
        return Some(config);
    }
    let config = path.join("config.toml");
    if config.exists() {
        return Some(config);
    }
    None
}

/// An iterator over cargo config paths.
#[allow(missing_debug_implementations)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct ConfigPaths<'a> {
    ancestors: std::path::Ancestors<'a>,
    cargo_home: Option<PathBuf>,
}

impl<'a> ConfigPaths<'a> {
    /// Creates an iterator over cargo config paths.
    pub fn new(current_dir: &'a Path) -> Self {
        Self {
            ancestors: current_dir.ancestors(),
            cargo_home: home::cargo_home_with_cwd(current_dir).ok(),
        }
    }
}

impl Iterator for ConfigPaths<'_> {
    type Item = PathBuf;
    fn next(&mut self) -> Option<Self::Item> {
        for p in self.ancestors.by_ref() {
            let p = p.join(".cargo");
            // dedup CARGO_HOME
            if self.cargo_home.as_ref() == Some(&p) {
                self.cargo_home = None;
            }
            if let Some(p) = config_path(&p) {
                return Some(p);
            }
        }
        config_path(&self.cargo_home.take()?)
    }
}
