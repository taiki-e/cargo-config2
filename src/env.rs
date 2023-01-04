// Environment variables are prefer over config values.
// https://doc.rust-lang.org/nightly/cargo/reference/config.html#environment-variables

use anyhow::{Context as _, Result};

use crate::{
    de::{
        BuildConfig, Config, DocConfig, FutureIncompatReportConfig, NetConfig, PathAndArgs,
        Rustflags, StringList, StringOrArray, TermConfig, TermProgress,
    },
    resolve::ResolveContext,
    value::{Definition, Value},
};

pub(crate) trait ApplyEnv {
    /// Applies configuration environment variables.
    ///
    /// Returns `Ok(true)` if this config is modified.
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool>;
}

impl<T: ApplyEnv + Default> ApplyEnv for Option<T> {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool> {
        match self {
            Some(v) => v.apply_env(cx),
            this @ None => {
                let mut v = T::default();
                if v.apply_env(cx)? {
                    *this = Some(v);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }
}

impl Config {
    /// Applies configuration environment variables.
    ///
    /// **Note:** This ignores environment variables for target-specific
    /// configurations ([`self.target`](Self::target). This is because it is
    /// difficult to determine exactly which target the target-specific
    /// configuration defined in the environment variables are for.
    /// (e.g., In environment variables, `-` and `.` in the target triple are replaced by `_`)
    pub fn apply_env(&mut self, cx: &ResolveContext) -> Result<()> {
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#alias
        for (k, v) in &cx.env {
            if let Some(k) = k.strip_prefix("CARGO_ALIAS_") {
                self.alias.insert(
                    k.to_owned(),
                    StringList::from_string(
                        v.to_str().ok_or_else(|| std::env::VarError::NotUnicode(v.clone()))?,
                        Some(&Definition::Environment(k.to_owned().into())),
                    ),
                );
                continue;
            }
        }

        // For self.target, we handle it in Config::resolve.

        self.build.apply_env(cx)?;
        self.doc.apply_env(cx)?;
        self.future_incompat_report.apply_env(cx)?;
        self.net.apply_env(cx)?;
        self.term.apply_env(cx)?;
        Ok(())
    }
}

impl ApplyEnv for BuildConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool> {
        let mut modified = false;

        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildjobs
        if let Some(jobs) = cx.env("CARGO_BUILD_JOBS")? {
            self.jobs = Some(jobs.parse()?);
            modified = true;
        }

        // The following priorities are not documented, but at as of cargo
        // 1.63.0-nightly (2022-05-31), `RUSTC*` are preferred over `CARGO_BUILD_RUSTC*`.
        // 1. RUSTC
        // 2. build.rustc (CARGO_BUILD_RUSTC)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc
        if let Some(rustc) = cx.env("RUSTC")? {
            self.rustc = Some(rustc);
            modified = true;
        } else if let Some(rustc) = cx.env("CARGO_BUILD_RUSTC")? {
            self.rustc = Some(rustc);
            modified = true;
        }
        // 1. RUSTC_WRAPPER
        // 2. build.rustc-wrapper (CARGO_BUILD_RUSTC_WRAPPER)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-wrapper
        if let Some(rustc_wrapper) = cx.env("RUSTC_WRAPPER")? {
            self.rustc_wrapper = Some(rustc_wrapper);
            modified = true;
        } else if let Some(rustc_wrapper) = cx.env("CARGO_BUILD_RUSTC_WRAPPER")? {
            self.rustc_wrapper = Some(rustc_wrapper);
            modified = true;
        }
        // 1. RUSTC_WORKSPACE_WRAPPER
        // 2. build.rustc-workspace-wrapper (CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustc-workspace-wrapper
        if let Some(rustc_workspace_wrapper) = cx.env("RUSTC_WORKSPACE_WRAPPER")? {
            self.rustc_workspace_wrapper = Some(rustc_workspace_wrapper);
            modified = true;
        } else if let Some(rustc_workspace_wrapper) =
            cx.env("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER")?
        {
            self.rustc_workspace_wrapper = Some(rustc_workspace_wrapper);
            modified = true;
        }
        // 1. RUSTDOC
        // 2. build.rustdoc (CARGO_BUILD_RUSTDOC)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdoc
        if let Some(rustdoc) = cx.env("RUSTDOC")? {
            self.rustdoc = Some(rustdoc);
            modified = true;
        } else if let Some(rustdoc) = cx.env("CARGO_BUILD_RUSTDOC")? {
            self.rustdoc = Some(rustdoc);
            modified = true;
        }

        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
        if let Some(target) = cx.env("CARGO_BUILD_TARGET")? {
            self.target = Some(StringOrArray::String(target));
            modified = true;
        }

        // The following priorities are not documented, but at as of cargo
        // 1.68.0-nightly (2022-12-23), `CARGO_*` are preferred over `CARGO_BUILD_*`.
        // 1. CARGO_TARGET_DIR
        // 2. CARGO_BUILD_TARGET_DIR
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildtarget
        if let Some(target_dir) = cx.env("CARGO_TARGET_DIR")? {
            self.target_dir = Some(target_dir);
            modified = true;
        } else if let Some(target_dir) = cx.env("CARGO_BUILD_TARGET_DIR")? {
            self.target_dir = Some(target_dir);
            modified = true;
        }

        // 1. CARGO_ENCODED_RUSTFLAGS
        // 2. RUSTFLAGS
        // 3. target.<triple>.rustflags (CARGO_TARGET_<triple>_RUSTFLAGS) and target.<cfg>.rustflags
        // 4. build.rustflags (CARGO_BUILD_RUSTFLAGS)
        // For 3, we handle it in Config::resolve.
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustflags
        self.override_target_rustflags = false;
        if let Some(rustflags) = cx.env("CARGO_ENCODED_RUSTFLAGS")? {
            self.rustflags = Some(Rustflags::from_encoded(&rustflags));
            self.override_target_rustflags = true;
            modified = true;
        } else if let Some(rustflags) = cx.env("RUSTFLAGS")? {
            self.rustflags = Some(Rustflags::from_space_separated(
                &rustflags.val,
                rustflags.definition.as_ref(),
            ));
            self.override_target_rustflags = true;
            modified = true;
        } else if let Some(rustflags) = cx.env("CARGO_BUILD_RUSTFLAGS")? {
            self.rustflags = Some(Rustflags::from_space_separated(
                &rustflags.val,
                rustflags.definition.as_ref(),
            ));
            modified = true;
        }
        // 1. CARGO_ENCODED_RUSTDOCFLAGS
        // 2. RUSTDOCFLAGS
        // 3. build.rustdocflags (CARGO_BUILD_RUSTDOCFLAGS)
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#buildrustdocflags
        if let Some(rustdocflags) = cx.env("CARGO_ENCODED_RUSTDOCFLAGS")? {
            self.rustdocflags = Some(Rustflags::from_encoded(&rustdocflags));
            modified = true;
        } else if let Some(rustdocflags) = cx.env("RUSTDOCFLAGS")? {
            self.rustdocflags = Some(Rustflags::from_space_separated(
                &rustdocflags.val,
                rustdocflags.definition.as_ref(),
            ));
            modified = true;
        } else if let Some(rustdocflags) = cx.env("CARGO_BUILD_RUSTDOCFLAGS")? {
            self.rustdocflags = Some(Rustflags::from_space_separated(
                &rustdocflags.val,
                rustdocflags.definition.as_ref(),
            ));
            modified = true;
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
            modified = true;
        } else if let Some(incremental) = cx.env("CARGO_BUILD_INCREMENTAL")? {
            self.incremental = Some(incremental.parse()?);
            modified = true;
        }

        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#builddep-info-basedir
        if let Some(dep_info_basedir) = cx.env("CARGO_BUILD_DEP_INFO_BASEDIR")? {
            self.dep_info_basedir = Some(dep_info_basedir);
            modified = true;
        }

        Ok(modified)
    }
}

impl ApplyEnv for DocConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool> {
        let mut modified = false;
        // doc.browser config value is prefer over BROWSER environment variable.
        // https://github.com/rust-lang/cargo/blob/0.67.0/src/cargo/ops/cargo_doc.rs#L52-L53
        if self.browser.is_none() {
            if let Some(browser) = cx.env("BROWSER")? {
                self.browser = Some(
                    PathAndArgs::from_string(&browser.val, browser.definition)
                        .context("invalid length 0, expected at least one element")?,
                );
                modified = true;
            }
        }
        Ok(modified)
    }
}

impl ApplyEnv for FutureIncompatReportConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool> {
        let mut modified = false;
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#future-incompat-reportfrequency
        if let Some(frequency) = cx.env("CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY")? {
            self.frequency = Some(frequency.parse()?);
            modified = true;
        }
        Ok(modified)
    }
}

impl ApplyEnv for NetConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool> {
        let mut modified = false;
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#netretry
        if let Some(retry) = cx.env("CARGO_NET_RETRY")? {
            self.retry = Some(retry.parse()?);
            modified = true;
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#netgit-fetch-with-cli
        if let Some(git_fetch_with_cli) = cx.env("CARGO_NET_GIT_FETCH_WITH_CLI")? {
            self.git_fetch_with_cli = Some(git_fetch_with_cli.parse()?);
            modified = true;
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#netoffline
        if let Some(offline) = cx.env("CARGO_NET_OFFLINE")? {
            self.offline = Some(offline.parse()?);
            modified = true;
        }
        Ok(modified)
    }
}

impl ApplyEnv for TermConfig {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool> {
        let mut modified = false;
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termquiet
        if let Some(quiet) = cx.env("CARGO_TERM_QUIET")? {
            self.quiet = Some(quiet.parse()?);
            modified = true;
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termverbose
        if let Some(verbose) = cx.env("CARGO_TERM_VERBOSE")? {
            self.verbose = Some(verbose.parse()?);
            modified = true;
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termcolor
        if let Some(color) = cx.env("CARGO_TERM_COLOR")? {
            self.color = Some(color.parse()?);
            modified = true;
        }
        modified |= self.progress.apply_env(cx)?;
        Ok(modified)
    }
}

impl ApplyEnv for TermProgress {
    fn apply_env(&mut self, cx: &ResolveContext) -> Result<bool> {
        let mut modified = false;
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswhen
        if let Some(when) = cx.env("CARGO_TERM_PROGRESS_WHEN")? {
            self.when = Some(when.parse()?);
            modified = true;
        }
        // https://doc.rust-lang.org/nightly/cargo/reference/config.html#termprogresswidth
        if let Some(width) = cx.env("CARGO_TERM_PROGRESS_WIDTH")? {
            self.width = Some(width.parse()?);
            modified = true;
        }
        Ok(modified)
    }
}
