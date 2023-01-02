use std::path::PathBuf;

use crate::de;

pub struct Config {
    de: de::Config,
    cargo_home: PathBuf,
    cwd: PathBuf,
}
