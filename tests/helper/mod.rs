// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    path::{Path, PathBuf},
    process::Command,
    str,
};

use anyhow::{bail, Context as _, Result};
pub use fs_err as fs;

pub fn fixtures_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

pub fn test_project(model: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    let tmpdir = tempfile::tempdir()?;
    let tmpdir_path = tmpdir.path();

    let model_path;
    let workspace_root;
    if model.contains('/') {
        let mut model = model.splitn(2, '/');
        model_path = fixtures_path().join(model.next().unwrap());
        workspace_root = tmpdir_path.join(model.next().unwrap());
        assert!(model.next().is_none());
    } else {
        model_path = fixtures_path().join(model);
        workspace_root = tmpdir_path.to_path_buf();
    }

    for (file_name, from) in git_ls_files(&model_path, &[])? {
        let to = &tmpdir_path.join(file_name);
        if !to.parent().unwrap().is_dir() {
            fs::create_dir_all(to.parent().unwrap())?;
        }
        fs::copy(from, to)?;
    }

    Ok((tmpdir, workspace_root))
}

fn git_ls_files(dir: &Path, filters: &[&str]) -> Result<Vec<(String, PathBuf)>> {
    let output = Command::new("git")
        .arg("ls-files")
        .args(filters)
        .current_dir(dir)
        .output()
        .with_context(|| format!("failed to run `git ls-files {filters:?}`"))?;
    if !output.status.success() {
        bail!("failed to run `git ls-files {filters:?}`");
    }
    Ok(str::from_utf8(&output.stdout)?
        .lines()
        .map(str::trim)
        .filter_map(|f| {
            if f.is_empty() {
                return None;
            }
            let p = dir.join(f);
            if !p.exists() {
                return None;
            }
            Some((f.to_owned(), p))
        })
        .collect())
}
