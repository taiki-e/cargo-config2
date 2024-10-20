// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    path::{Path, PathBuf},
    process::Command,
    str,
};

pub(crate) use fs_err as fs;

pub(crate) fn fixtures_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

#[track_caller]
pub(crate) fn test_project(model: &str) -> (tempfile::TempDir, PathBuf) {
    let tmpdir = tempfile::tempdir().unwrap();
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

    for (file_name, from) in git_ls_files(&model_path, &[]) {
        let to = &tmpdir_path.join(file_name);
        if !to.parent().unwrap().is_dir() {
            fs::create_dir_all(to.parent().unwrap()).unwrap();
        }
        fs::copy(from, to).unwrap();
    }

    (tmpdir, workspace_root)
}

#[track_caller]
fn git_ls_files(dir: &Path, filters: &[&str]) -> Vec<(String, PathBuf)> {
    let mut cmd = Command::new("git");
    cmd.arg("ls-files").args(filters).current_dir(dir);
    let output =
        cmd.output().unwrap_or_else(|e| panic!("could not execute process `{cmd:?}`: {e}"));
    assert!(
        output.status.success(),
        "process didn't exit successfully: `{cmd:?}`:\n\nSTDOUT:\n{0}\n{1}\n{0}\n\nSTDERR:\n{0}\n{2}\n{0}\n",
        "-".repeat(60),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    str::from_utf8(&output.stdout)
        .unwrap()
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
        .collect()
}
