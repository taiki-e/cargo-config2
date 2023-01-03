use std::{mem, path::PathBuf};

use anyhow::Result;
use once_cell::unsync::OnceCell;

use crate::{de, easy, ResolveContext};

pub struct Config {
    de: de::Config,
    cargo_home: PathBuf,
    cwd: PathBuf,
    cx: ResolveContext,

    build: Option<easy::BuildConfig>,
}
