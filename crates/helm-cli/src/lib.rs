//! Command-line frontend for Helm.

#![forbid(unsafe_code)]

use std::{
    fmt::Write as _,
    io::{self, Write},
};

use clap::{Parser, Subcommand, ValueEnum};
use helm_core::{DiscoveryService, SystemProbe, foundation_catalog};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, Parser)]
#[command(name = "helm-settings", version, about = "Safe settings control plane")]
pub struct Cli {
    #[arg(long, global = true, value_enum, default_value_t)]
    output: OutputFormat,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Discover the current desktop and supported integrations.
    Discover,
    /// Show a concise health summary.
    Status,
    /// Inspect the canonical setting catalog.
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SettingsCommand {
    List,
}

#[derive(Serialize)]
struct Envelope<T> {
    schema_version: u32,
    ok: bool,
    data: T,
    warnings: Vec<String>,
}

pub fn run_from<I, T>(arguments: I, output: &mut impl Write) -> Result<(), String>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(arguments).map_err(|error| error.to_string())?;
    let report = DiscoveryService::new(SystemProbe).discover();
    match cli.command.unwrap_or(Command::Status) {
        Command::Discover => write_value(cli.output, output, &report, || {
            let mut text = format!("Session: {}\n", report.session);
            for component in &report.components {
                writeln!(
                    text,
                    "{:<12} {:?}{}",
                    component.display_name,
                    component.availability,
                    component
                        .version
                        .as_deref()
                        .map_or_else(String::new, |v| format!(" ({v})"))
                )
                .expect("writing to a String cannot fail");
            }
            text
        }),
        Command::Status => {
            let available = report
                .components
                .iter()
                .filter(|component| {
                    component.availability == helm_core::model::Availability::Available
                })
                .count();
            let summary = serde_json::json!({ "session": report.session, "available_components": available, "total_components": report.components.len() });
            write_value(cli.output, output, &summary, || {
                format!(
                    "Helm: {available}/{} supported components available\n",
                    report.components.len()
                )
            })
        }
        Command::Settings {
            command: SettingsCommand::List,
        } => {
            let catalog = foundation_catalog();
            write_value(cli.output, output, &catalog, || {
                catalog.iter().fold(String::new(), |mut text, setting| {
                    writeln!(text, "{}\t{}", setting.id.0, setting.label)
                        .expect("writing to a String cannot fail");
                    text
                })
            })
        }
    }
}

fn write_value<T: Serialize>(
    format: OutputFormat,
    output: &mut impl Write,
    value: &T,
    human: impl FnOnce() -> String,
) -> Result<(), String> {
    match format {
        OutputFormat::Human => output
            .write_all(human().as_bytes())
            .map_err(|error| error.to_string()),
        OutputFormat::Json => {
            let envelope = Envelope {
                schema_version: 1,
                ok: true,
                data: value,
                warnings: Vec::new(),
            };
            serde_json::to_writer(&mut *output, &envelope).map_err(|error| error.to_string())?;
            output.write_all(b"\n").map_err(|error| error.to_string())
        }
    }
}

pub fn run() -> Result<(), String> {
    run_from(std::env::args_os(), &mut io::stdout().lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_json_has_versioned_envelope() {
        let mut output = Vec::new();
        run_from(
            ["helm-settings", "--output", "json", "settings", "list"],
            &mut output,
        )
        .expect("command succeeds");
        let value: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], true);
        assert!(
            value["data"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
    }
}
