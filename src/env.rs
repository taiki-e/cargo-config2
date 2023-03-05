// Environment variables are prefer over config values.
// https://doc.rust-lang.org/nightly/cargo/reference/config.html#environment-variables

use crate::{
    de::{
        BuildConfig, Config, DocConfig, Flags, FutureIncompatReportConfig, NetConfig, PathAndArgs,
        RegistriesConfigValue, RegistryConfig, StringList, StringOrArray, TermConfig, TermProgress,
    },
    error::{Context as _, Error, Result},
    resolve::ResolveContext,
    value::{Definition, Value},
};

pub(crate) trait ApplyEnv {
    /// Applies configuration environment variables.
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()>;
}

impl Config {
    /// Applies configuration environment variables.
    ///
    /// **Note:** This ignores environment variables for target-specific
    /// configurations ([`self.target`](Self::target). This is because it is
    /// difficult to determine exactly which target the target-specific
    /// configuration defined in the environment variables are for.
    /// (e.g., In environment variables, `-` and `.` in the target triple are replaced by `_`)
    #[doc(hidden)] // Not public API.
    pub fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        for (k, v) in &cx.env {
            let definition = || Some(Definition::Environment(k.clone().into()));
            let error_env_not_unicode = || Error::env_not_unicode(k, v.clone());

            // https://doc.rust-lang.org/nightly/cargo/reference/config.html#alias
            if let Some(k) = k.strip_prefix("CARGO_ALIAS_") {
                self.alias.insert(
                    k.to_owned(),
                    StringList::from_string(
                        v.to_str().ok_or_else(error_env_not_unicode)?,
                        definition().as_ref(),
                    ),
                );
                continue;
            }
            // https://doc.rust-lang.org/nightly/cargo/reference/config.html#registries
            else if let Some(k) = k.strip_prefix("CARGO_REGISTRIES_") {
                if let Some(k) = k.strip_suffix("_INDEX") {
                    let v = v.to_str().ok_or_else(error_env_not_unicode)?;
                    let index = Some(Value { val: v.to_owned(), definition: definition() });
                    if let Some(registries_config_value) = self.registries.get_mut(k) {
                        registries_config_value.index = index;
                    } else {
                        self.registries.insert(k.to_owned(), RegistriesConfigValue {
                            index,
                            token: None,
                            protocol: None,
                        });
                    }
                    continue;
                } else if let Some(k) = k.strip_suffix("_TOKEN") {
                    let v = v.to_str().ok_or_else(error_env_not_unicode)?;
                    let token = Some(Value { val: v.to_owned(), definition: definition() });
                    if let Some(registries_config_value) = self.registries.get_mut(k) {
                        registries_config_value.token = token;
                    } else {
                        self.registries.insert(k.to_owned(), RegistriesConfigValue {
                            index: None,
                            token,
                            protocol: None,
                        });
                    }
                    continue;
                } else if k == "CRATES_IO_PROTOCOL" {
                    let k = "crates-io";
                    let v = v.to_str().ok_or_else(error_env_not_unicode)?;
                    let protocol =
                        Some(Value { val: v.to_owned(), definition: definition() }.parse()?);
                    if let Some(registries_config_value) = self.registries.get_mut(k) {
                        registries_config_value.protocol = protocol;
                    } else {
                        self.registries.insert(k.to_owned(), RegistriesConfigValue {
                            index: None,
                            token: None,
                            protocol,
                        });
                    }
                    continue;
                }
            }
        }

        // For self.target, we handle it in Config::resolve.

        self.build.apply_env(cx)?;
        self.doc.apply_env(cx)?;
        self.future_incompat_report.apply_env(cx)?;
        self.net.apply_env(cx)?;
        self.registry.apply_env(cx)?;
        self.term.apply_env(cx)?;
        Ok(())
    }
}

impl ApplyEnv for BuildConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildjobs
        if let Some(jobs) = cx.env_parse("CARGO_BUILD_JOBS")? {
            self.jobs = Some(jobs);
        }

        // The following priorities are not documented, but at as of cargo
        // 1.63.0-nightly (2022-05-31), `RUSTC*` are preferred over `CARGO_BUILD_RUSTC*`.
        // 1. RUSTC
        // 2. build.rustc (CARGO_BUILD_RUSTC)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc
        // See also https://github.com/taiki-e/cargo-llvm-cov/pull/180#discussion_r887904341.
        if let Some(rustc) = cx.env("RUSTC")? {
            self.rustc = Some(rustc);
        } else if let Some(rustc) = cx.env("CARGO_BUILD_RUSTC")? {
            self.rustc = Some(rustc);
        }
        // 1. RUSTC_WRAPPER
        // 2. build.rustc-wrapper (CARGO_BUILD_RUSTC_WRAPPER)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-wrapper
        // Setting this to an empty string instructs cargo to not use a wrapper.
        // https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-reads
        if let Some(rustc_wrapper) = cx.env("RUSTC_WRAPPER")? {
            if rustc_wrapper.val.is_empty() {
                self.rustc_wrapper = None;
            } else {
                self.rustc_wrapper = Some(rustc_wrapper);
            }
        } else if let Some(rustc_wrapper) = cx.env("CARGO_BUILD_RUSTC_WRAPPER")? {
            if rustc_wrapper.val.is_empty() {
                self.rustc_wrapper = None;
            } else {
                self.rustc_wrapper = Some(rustc_wrapper);
            }
        }
        // 1. RUSTC_WORKSPACE_WRAPPER
        // 2. build.rustc-workspace-wrapper (CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-workspace-wrapper
        // Setting this to an empty string instructs cargo to not use a wrapper.
        // https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-reads
        if let Some(rustc_workspace_wrapper) = cx.env("RUSTC_WORKSPACE_WRAPPER")? {
            if rustc_workspace_wrapper.val.is_empty() {
                self.rustc_workspace_wrapper = None;
            } else {
                self.rustc_workspace_wrapper = Some(rustc_workspace_wrapper);
            }
        } else if let Some(rustc_workspace_wrapper) =
            cx.env("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER")?
        {
            if rustc_workspace_wrapper.val.is_empty() {
                self.rustc_workspace_wrapper = None;
            } else {
                self.rustc_workspace_wrapper = Some(rustc_workspace_wrapper);
            }
        }
        // 1. RUSTDOC
        // 2. build.rustdoc (CARGO_BUILD_RUSTDOC)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdoc
        if let Some(rustdoc) = cx.env("RUSTDOC")? {
            self.rustdoc = Some(rustdoc);
        } else if let Some(rustdoc) = cx.env("CARGO_BUILD_RUSTDOC")? {
            self.rustdoc = Some(rustdoc);
        }

        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
        if let Some(target) = cx.env("CARGO_BUILD_TARGET")? {
            self.target = Some(StringOrArray::String(target));
        }

        // The following priorities are not documented, but at as of cargo
        // 1.68.0-nightly (2022-12-23), `CARGO_*` are preferred over `CARGO_BUILD_*`.
        // 1. CARGO_TARGET_DIR
        // 2. CARGO_BUILD_TARGET_DIR
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
        if let Some(target_dir) = cx.env("CARGO_TARGET_DIR")? {
            self.target_dir = Some(target_dir);
        } else if let Some(target_dir) = cx.env("CARGO_BUILD_TARGET_DIR")? {
            self.target_dir = Some(target_dir);
        }

        // 1. CARGO_ENCODED_RUSTFLAGS
        // 2. RUSTFLAGS
        // 3. target.<triple>.rustflags (CARGO_TARGET_<triple>_RUSTFLAGS) and target.<cfg>.rustflags
        // 4. build.rustflags (CARGO_BUILD_RUSTFLAGS)
        // For 3, we handle it in Config::resolve.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
        self.override_target_rustflags = false;
        if let Some(rustflags) = cx.env("CARGO_ENCODED_RUSTFLAGS")? {
            self.rustflags = Some(Flags::from_encoded(&rustflags));
            self.override_target_rustflags = true;
        } else if let Some(rustflags) = cx.env("RUSTFLAGS")? {
            self.rustflags =
                Some(Flags::from_space_separated(&rustflags.val, rustflags.definition.as_ref()));
            self.override_target_rustflags = true;
        } else if let Some(rustflags) = cx.env("CARGO_BUILD_RUSTFLAGS")? {
            self.rustflags =
                Some(Flags::from_space_separated(&rustflags.val, rustflags.definition.as_ref()));
        }
        // 1. CARGO_ENCODED_RUSTDOCFLAGS
        // 2. RUSTDOCFLAGS
        // 3. build.rustdocflags (CARGO_BUILD_RUSTDOCFLAGS)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
        if let Some(rustdocflags) = cx.env("CARGO_ENCODED_RUSTDOCFLAGS")? {
            self.rustdocflags = Some(Flags::from_encoded(&rustdocflags));
        } else if let Some(rustdocflags) = cx.env("RUSTDOCFLAGS")? {
            self.rustdocflags = Some(Flags::from_space_separated(
                &rustdocflags.val,
                rustdocflags.definition.as_ref(),
            ));
        } else if let Some(rustdocflags) = cx.env("CARGO_BUILD_RUSTDOCFLAGS")? {
            self.rustdocflags = Some(Flags::from_space_separated(
                &rustdocflags.val,
                rustdocflags.definition.as_ref(),
            ));
        }

        // The following priorities are not documented, but at as of cargo
        // 1.68.0-nightly (2022-12-23), `CARGO_*` are preferred over `CARGO_BUILD_*`.
        // 1. CARGO_INCREMENTAL
        // 2. CARGO_BUILD_INCREMENTAL
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildincremental
        if let Some(incremental) = cx.env("CARGO_INCREMENTAL")? {
            // As of cargo 1.68.0-nightly (2022-12-23), cargo handles invalid value like 0.
            self.incremental =
                Some(Value { val: incremental.val == "1", definition: incremental.definition });
        } else if let Some(incremental) = cx.env_parse("CARGO_BUILD_INCREMENTAL")? {
            self.incremental = Some(incremental);
        }

        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#builddep-info-basedir
        if let Some(dep_info_basedir) = cx.env("CARGO_BUILD_DEP_INFO_BASEDIR")? {
            self.dep_info_basedir = Some(dep_info_basedir);
        }

        Ok(())
    }
}

impl ApplyEnv for DocConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // doc.browser config value is prefer over BROWSER environment variable.
        // https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/ops/cargo_doc.rs#L52-L53
        if self.browser.is_none() {
            if let Some(browser) = cx.env("BROWSER")? {
                self.browser = Some(
                    PathAndArgs::from_string(&browser.val, browser.definition)
                        .context("invalid length 0, expected at least one element")?,
                );
            }
        }
        Ok(())
    }
}

impl ApplyEnv for FutureIncompatReportConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#future-incompat-reportfrequency
        if let Some(frequency) = cx.env_parse("CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY")? {
            self.frequency = Some(frequency);
        }
        Ok(())
    }
}

impl ApplyEnv for NetConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#netretry
        if let Some(retry) = cx.env_parse("CARGO_NET_RETRY")? {
            self.retry = Some(retry);
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#netgit-fetch-with-cli
        if let Some(git_fetch_with_cli) = cx.env_parse("CARGO_NET_GIT_FETCH_WITH_CLI")? {
            self.git_fetch_with_cli = Some(git_fetch_with_cli);
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#netoffline
        if let Some(offline) = cx.env_parse("CARGO_NET_OFFLINE")? {
            self.offline = Some(offline);
        }
        Ok(())
    }
}

impl ApplyEnv for RegistryConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#registrydefault
        if let Some(default) = cx.env_parse("CARGO_REGISTRY_DEFAULT")? {
            self.default = Some(default);
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#registrytoken
        if let Some(token) = cx.env_parse("CARGO_REGISTRY_TOKEN")? {
            self.token = Some(token);
        }
        Ok(())
    }
}

impl ApplyEnv for TermConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termquiet
        if let Some(quiet) = cx.env_parse("CARGO_TERM_QUIET")? {
            self.quiet = Some(quiet);
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termverbose
        if let Some(verbose) = cx.env_parse("CARGO_TERM_VERBOSE")? {
            self.verbose = Some(verbose);
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termcolor
        if let Some(color) = cx.env_parse("CARGO_TERM_COLOR")? {
            self.color = Some(color);
        }
        self.progress.apply_env(cx)?;
        Ok(())
    }
}

impl ApplyEnv for TermProgress {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswhen
        if let Some(when) = cx.env_parse("CARGO_TERM_PROGRESS_WHEN")? {
            self.when = Some(when);
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswidth
        if let Some(width) = cx.env_parse("CARGO_TERM_PROGRESS_WIDTH")? {
            self.width = Some(width);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ApplyEnv;
    use crate::{value::Value, ResolveOptions};

    #[test]
    fn empty_string_wrapper_envs() {
        let env_list = [("RUSTC_WRAPPER", ""), ("RUSTC_WORKSPACE_WRAPPER", "")];
        let mut config = crate::de::BuildConfig::default();
        let cx = &ResolveOptions::default().env(env_list).into_context();
        config.rustc_wrapper = Some(Value { val: "rustc_wrapper".to_string(), definition: None });
        config.rustc_workspace_wrapper =
            Some(Value { val: "rustc_workspace_wrapper".to_string(), definition: None });
        config.apply_env(cx).unwrap();
        assert!(config.rustc_wrapper.is_none());
        assert!(config.rustc_workspace_wrapper.is_none());
    }
}
