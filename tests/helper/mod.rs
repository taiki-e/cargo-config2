// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    path::{Path, PathBuf},
    str,
};

pub(crate) use fs_err as fs;

pub(crate) fn fixtures_dir() -> &'static Path {
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
        model_path = fixtures_dir().join(model.next().unwrap());
        workspace_root = tmpdir_path.join(model.next().unwrap());
        assert!(model.next().is_none());
    } else {
        model_path = fixtures_dir().join(model);
        workspace_root = tmpdir_path.to_path_buf();
    }

    for (file_name, from) in test_helper::git::ls_files(model_path, &[]) {
        let to = &tmpdir_path.join(file_name);
        if !to.parent().unwrap().is_dir() {
            fs::create_dir_all(to.parent().unwrap()).unwrap();
        }
        fs::copy(from, to).unwrap();
    }

    (tmpdir, workspace_root)
}
