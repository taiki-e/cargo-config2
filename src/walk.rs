// SPDX-License-Identifier: Apache-2.0 OR MIT

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

/// An iterator over Cargo configuration file paths.
#[derive(Debug)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Walk<'a> {
    ancestors: std::path::Ancestors<'a>,
    cargo_home: Option<PathBuf>,
}

impl<'a> Walk<'a> {
    /// Creates an iterator over Cargo configuration file paths from the given path.
    pub fn new(current_dir: &'a Path) -> Self {
        Self::with_cargo_home(current_dir, home::cargo_home_with_cwd(current_dir).ok())
    }

    /// Creates an iterator over Cargo configuration file paths from the given path
    /// and `CARGO_HOME` path.
    pub fn with_cargo_home(current_dir: &'a Path, cargo_home: Option<PathBuf>) -> Self {
        Self { ancestors: current_dir.ancestors(), cargo_home }
    }
}

impl Iterator for Walk<'_> {
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

#[cfg(test)]
mod tests {
    use fs_err as fs;

    use super::*;

    #[test]
    fn walk() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        let home = &p.join("a/.cargo");
        let cwd = &p.join("a/b/c");
        fs::create_dir_all(home).unwrap();
        fs::write(p.join("a/.cargo/config"), "").unwrap();
        fs::create_dir_all(p.join("a/b/.cargo")).unwrap();
        fs::write(p.join("a/b/.cargo/config"), "").unwrap();
        fs::write(p.join("a/b/.cargo/config.toml"), "").unwrap();
        fs::create_dir_all(p.join("a/b/c/.cargo")).unwrap();
        fs::write(p.join("a/b/c/.cargo/config.toml"), "").unwrap();
        let mut w = Walk::with_cargo_home(cwd, Some(home.clone()));
        assert_eq!(w.next(), Some(p.join("a/b/c/.cargo/config.toml")));
        assert_eq!(w.next(), Some(p.join("a/b/.cargo/config")));
        assert_eq!(w.next(), Some(p.join("a/.cargo/config")));
        assert_eq!(w.next(), None);
    }
}
