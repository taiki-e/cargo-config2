// Partial re-implementation of `cargo config get` using cargo-config2.

use std::{
    env,
    io::{self, Write},
    str::FromStr,
};

use anyhow::{bail, Result};
use cargo_config2::Config;
use lexopt::{
    Arg::{Long, Short},
    ValueExt,
};

// TODO: --show-origin and --config
static USAGE:&str = "cargo-config2-get
Usage: cargo run --example get -- [OPTIONS]

Options:
      --format <format>     Display format [default: toml] [possible values: toml, json]
      --merged <merged>     Whether or not to merge config values [default: yes] [possible values: yes, no]
  -h, --help                Print help information
";

fn main() {
    if let Err(e) = try_main() {
        eprintln!("error: {e:#}");
        std::process::exit(1)
    }
}

fn try_main() -> Result<()> {
    let args = Args::parse()?;

    let mut stdout = io::stdout().lock();
    match args.merged {
        Merged::Yes => {
            let config = Config::load()?;
            print_config(&mut stdout, args.format, &config)?;
        }
        Merged::No => {
            if args.format == Format::Json {
                bail!(
                    "the `json` format does not support --merged=no, try the `toml` format instead"
                );
            }
            for config in Config::load_unmerged()? {
                writeln!(stdout, "# {}", config.path().unwrap().display())?;
                print_config(&mut stdout, args.format, &config)?;
                writeln!(stdout)?;
            }
        }
    }
    stdout.flush()?;

    // In toml format, `cargo config get` outputs this in the form of a comment,
    // but may output toml in an invalid format because it does not handle newlines properly.
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "note: The following environment variables may affect the loaded values.")?;
    for (k, v) in std::env::vars_os() {
        if let (Ok(k), Ok(v)) = (k.into_string(), v.into_string()) {
            if k.starts_with("CARGO_") {
                writeln!(stderr, "{k}={}", shell_escape::escape(v.into()))?;
            }
        }
    }
    stderr.flush()?;

    Ok(())
}

fn print_config(writer: &mut dyn Write, format: Format, config: &Config) -> Result<()> {
    match format {
        Format::Json => writeln!(writer, "{}", serde_json::to_string(&config)?)?,
        Format::Toml => {
            // `cargo config get` displays config with the following format:
            //
            // ```
            // a.b.c = <value>
            // a.b.d = <value>
            // ```
            //
            // Neither toml nor toml_edit supports this output format, so format it manually.
            fn print_item(
                writer: &mut dyn Write,
                path: &str,
                item: &toml_edit::Item,
            ) -> Result<()> {
                match item {
                    toml_edit::Item::None => {}
                    toml_edit::Item::Value(value) => print_value(writer, path, value)?,
                    toml_edit::Item::Table(table) => {
                        for (key, item) in table.iter() {
                            print_item(writer, &format!("{path}.{key}"), item)?;
                        }
                    }
                    toml_edit::Item::ArrayOfTables(_) => todo!(),
                }
                Ok(())
            }
            fn print_value(
                writer: &mut dyn Write,
                path: &str,
                value: &toml_edit::Value,
            ) -> Result<()> {
                match value {
                    toml_edit::Value::InlineTable(table) => {
                        for (key, value) in table.iter() {
                            print_value(writer, &format!("{path}.{key}"), value)?;
                        }
                    }
                    _ => writeln!(writer, "{path} = {value}")?,
                }
                Ok(())
            }
            let doc = toml_edit::easy::to_document(&config)?;
            for (key, item) in doc.iter() {
                print_item(writer, key, item)?;
            }
        }
    }
    Ok(())
}

struct Args {
    format: Format,
    merged: Merged,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Format {
    #[default]
    Toml,
    Json,
}

impl FromStr for Format {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "toml" => Ok(Self::Toml),
            "json" => Ok(Self::Json),
            other => bail!("must be toml or json, but found `{other}`"),
        }
    }
}

#[derive(Clone, Copy, Default)]
enum Merged {
    #[default]
    Yes,
    No,
}

impl FromStr for Merged {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "yes" => Ok(Self::Yes),
            "no" => Ok(Self::No),
            other => bail!("must be yes or no, but found `{other}`"),
        }
    }
}

impl Args {
    fn parse() -> Result<Self> {
        let mut format: Option<Format> = None;
        let mut merged: Option<Merged> = None;

        let mut parser = lexopt::Parser::from_env();
        while let Some(arg) = parser.next()? {
            match arg {
                Long("format") if format.is_none() => format = Some(parser.value()?.parse()?),
                Long("merged") if merged.is_none() => merged = Some(parser.value()?.parse()?),
                Short('h') | Long("help") => {
                    print!("{USAGE}");
                    std::process::exit(0);
                }
                Short('V') | Long("version") => {
                    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                _ => return Err(arg.unexpected().into()),
            }
        }

        Ok(Self { format: format.unwrap_or_default(), merged: merged.unwrap_or_default() })
    }
}
