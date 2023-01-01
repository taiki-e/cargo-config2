use std::ffi::OsStr;

use anyhow::{format_err, Result};

/// Gets host triple of the given `rustc` or `cargo`.
///
/// # Examples
///
/// ```
/// # fn main() -> anyhow::Result<()> {
/// let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
/// let host = cargo_config2::host_triple(cargo)?;
/// # Ok(()) }
/// ```
pub fn host_triple(rustc_or_cargo: impl AsRef<OsStr>) -> Result<String> {
    fn inner(rustc_or_cargo: &OsStr) -> Result<String> {
        let mut cmd = cmd!(rustc_or_cargo, "--version", "--verbose");
        let verbose_version = cmd.read()?;
        let host = verbose_version
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or_else(|| {
                format_err!("unexpected version output from `{cmd}`: {verbose_version}")
            })?
            .to_owned();
        Ok(host)
    }
    inner(rustc_or_cargo.as_ref())
}
