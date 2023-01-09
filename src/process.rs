use std::{
    ffi::OsString,
    fmt,
    process::{Command, ExitStatus, Output},
    str,
};

use shell_escape::escape;

use crate::{
    error::{Context as _, Result},
    Error,
};

macro_rules! cmd {
    ($program:expr $(, $arg:expr)* $(,)?) => {{
        let mut _cmd = $crate::process::ProcessBuilder::new($program);
        $(
            _cmd.arg($arg);
        )*
        _cmd
    }};
}

// A builder for an external process, inspired by https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/process_builder.rs
#[must_use]
#[derive(Debug, Clone)]
pub(crate) struct ProcessBuilder {
    /// The program to execute.
    program: OsString,
    /// A list of arguments to pass to the program.
    args: Vec<OsString>,
}

impl ProcessBuilder {
    /// Creates a new `ProcessBuilder`.
    pub(crate) fn new(program: impl Into<OsString>) -> Self {
        Self { program: program.into(), args: Vec::new() }
    }

    /// Adds an argument to pass to the program.
    pub(crate) fn arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    /// Adds multiple arguments to pass to the program.
    pub(crate) fn args(
        &mut self,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> &mut Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Executes a process, captures its stdio output, returning the captured
    /// output, or an error if non-zero exit status.
    pub(crate) fn run_with_output(&mut self) -> Result<Output> {
        let output = self.build().output().with_context(|| {
            ProcessError::new(&format!("could not execute process {self}"), None, None)
        })?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(Error::new(ProcessError::new(
                &format!("process didn't exit successfully: {self}"),
                Some(output.status),
                Some(&output),
            )))
        }
    }

    /// Executes a process, captures its stdio output, returning the captured
    /// standard output as a `String`.
    pub(crate) fn read(&mut self) -> Result<String> {
        let mut output = String::from_utf8(self.run_with_output()?.stdout)
            .with_context(|| format!("failed to parse output from {self}"))?;
        while output.ends_with('\n') || output.ends_with('\r') {
            output.pop();
        }
        Ok(output)
    }

    pub(crate) fn build(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd
    }
}

// Based on https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/process_builder.rs
impl fmt::Display for ProcessBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !f.alternate() {
            write!(f, "`")?;
        }

        write!(f, "{}", self.program.to_string_lossy())?;

        for arg in &self.args {
            write!(f, " {}", escape(arg.to_string_lossy()))?;
        }

        if !f.alternate() {
            write!(f, "`")?;
        }

        Ok(())
    }
}

// Based on https://github.com/rust-lang/cargo/blob/0.47.0/src/cargo/util/errors.rs
#[derive(Debug)]
pub(crate) struct ProcessError {
    /// A detailed description to show to the user why the process failed.
    desc: String,
}

impl ProcessError {
    /// Creates a new process error.
    ///
    /// `status` can be `None` if the process did not launch.
    /// `output` can be `None` if the process did not launch, or output was not captured.
    fn new(msg: &str, status: Option<ExitStatus>, output: Option<&Output>) -> Self {
        let exit = match status {
            Some(s) => s.to_string(),
            None => "never executed".to_string(),
        };
        let mut desc = format!("{} ({exit})", &msg);

        if let Some(out) = output {
            match str::from_utf8(&out.stdout) {
                Ok(s) if !s.trim().is_empty() => {
                    desc.push_str("\n--- stdout\n");
                    desc.push_str(s);
                }
                Ok(_) | Err(_) => {}
            }
            match str::from_utf8(&out.stderr) {
                Ok(s) if !s.trim().is_empty() => {
                    desc.push_str("\n--- stderr\n");
                    desc.push_str(s);
                }
                Ok(_) | Err(_) => {}
            }
        }

        Self { desc }
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.desc, f)
    }
}

impl std::error::Error for ProcessError {}
