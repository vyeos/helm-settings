//! Command-line frontend for Helm.

#![forbid(unsafe_code)]

use std::{
    fmt::Write as _,
    io::{self, Write},
};

use clap::{Parser, Subcommand, ValueEnum};
use helm_adapter_applications::{alacritty, theme, yazi};
use helm_adapter_bars::{Layout as BarLayout, quickshell, waybar};
use helm_adapter_desktop::profile::Profile;
use helm_adapter_hyprland::{HyprlandRuntime, ProcessRuntime, detect_generation};
use helm_core::{DiscoveryService, SystemProbe, XdgPaths, foundation_catalog};
use helm_transaction::{Engine, FileChange, Fingerprint, TransactionPlan};
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
    /// Inspect or restore durable configuration history.
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Inspect the active Hyprland adapter without mutating configuration.
    Hyprland {
        #[command(subcommand)]
        command: HyprlandCommand,
    },
    /// Inspect and configure supported applications and shared themes.
    Applications {
        #[command(subcommand)]
        command: ApplicationsCommand,
    },
    /// Inspect and configure managed bar integrations.
    Bars {
        #[command(subcommand)]
        command: BarsCommand,
    },
    /// Validate or atomically apply a desired-state profile.
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SettingsCommand {
    List,
}

#[derive(Debug, Subcommand)]
enum HistoryCommand {
    /// List recent transactions.
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Restore a committed transaction as a new transaction.
    Undo { transaction_id: String },
}

#[derive(Clone, Copy, Debug, Subcommand)]
enum HyprlandCommand {
    Options,
    Displays,
    Bindings,
    ConfigStatus,
}

#[derive(Debug, Subcommand)]
enum ApplicationsCommand {
    Themes,
    AlacrittyStatus,
    AlacrittyTheme { theme_id: String },
    YaziFlavors,
    YaziFlavor { flavor_id: String },
}

#[derive(Clone, Copy, Debug, Subcommand)]
enum BarsCommand {
    Status,
    ApplyWaybar,
    ApplyQuickshell,
}

#[derive(Debug, Subcommand)]
enum ProfilesCommand {
    Validate { path: std::path::PathBuf },
    Apply { path: std::path::PathBuf },
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
        Command::History { command } => {
            let engine = default_engine()?;
            match command {
                HistoryCommand::List { limit } => {
                    let history = engine.history(limit).map_err(|error| error.to_string())?;
                    write_value(cli.output, output, &history, || {
                        history.iter().fold(String::new(), |mut text, entry| {
                            writeln!(text, "{}\t{:?}\t{}", entry.id, entry.state, entry.summary)
                                .expect("writing to a String cannot fail");
                            text
                        })
                    })
                }
                HistoryCommand::Undo { transaction_id } => {
                    let result = engine
                        .undo(&transaction_id, || Ok(()))
                        .map_err(|error| error.to_string())?;
                    write_value(cli.output, output, &result, || {
                        format!("Restored as transaction {}\n", result.id)
                    })
                }
            }
        }
        Command::Hyprland { command } => run_hyprland(command, cli.output, output),
        Command::Applications { command } => run_applications(command, cli.output, output),
        Command::Bars { command } => run_bars(command, cli.output, output),
        Command::Profiles { command } => run_profiles(&command, cli.output, output),
    }
}

fn run_profiles(
    command: &ProfilesCommand,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    let path = match &command {
        ProfilesCommand::Validate { path } | ProfilesCommand::Apply { path } => path,
    };
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read profile {}: {error}", path.display()))?;
    let profile: Profile =
        serde_json::from_str(&source).map_err(|error| format!("invalid profile JSON: {error}"))?;
    profile.validate().map_err(|error| error.to_string())?;
    match command {
        ProfilesCommand::Validate { .. } => write_value(format, output, &profile, || {
            format!("Profile `{}` is valid\n", profile.name)
        }),
        ProfilesCommand::Apply { .. } => apply_profile(&profile, format, output),
    }
}

fn apply_profile(
    profile: &Profile,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    let paths = XdgPaths::from_environment().map_err(str::to_owned)?;
    for wallpaper in &profile.wallpapers {
        if !wallpaper.path.is_file() {
            return Err(format!(
                "wallpaper does not exist or is not a file: {}",
                wallpaper.path.display()
            ));
        }
    }
    let mimeapps_path = paths.config_home.join("mimeapps.list");
    let hyprpaper_path = paths.config_home.join("hypr/hyprpaper.conf");
    let hyprland_path = paths.config_home.join("hypr/hyprland.lua");
    let helm_init_path = paths.config_home.join("hypr/helm-settings/init.lua");
    let files = profile
        .render_files(
            &paths.config_home,
            &read_optional_text(&mimeapps_path)?,
            &read_optional_text(&hyprpaper_path)?,
            &read_optional_text(&hyprland_path)?,
            &read_optional_text(&helm_init_path)?,
        )
        .map_err(|error| error.to_string())?;
    let expected = files
        .iter()
        .map(|file| (file.path.clone(), file.content.clone()))
        .collect::<Vec<_>>();
    let changes = files
        .into_iter()
        .map(|file| write_change(&file.path, file.content.into_bytes()))
        .collect::<Result<Vec<_>, _>>()?;
    let transaction = TransactionPlan {
        summary: format!("Apply profile: {}", profile.name),
        changes,
    };
    let result = default_engine()?
        .apply(&transaction, || {
            for (path, expected) in &expected {
                let actual = std::fs::read_to_string(path)
                    .map_err(|error| format!("cannot verify {}: {error}", path.display()))?;
                if &actual != expected {
                    return Err(format!("{} changed during profile apply", path.display()));
                }
            }
            verify_active_profile(profile)
        })
        .map_err(|error| error.to_string())?;
    write_value(format, output, &result, || {
        format!(
            "Applied profile `{}` as transaction {}\n",
            profile.name, result.id
        )
    })
}

fn verify_active_profile(profile: &Profile) -> Result<(), String> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Ok(());
    }
    if !profile.window_rules.is_empty() {
        let status = std::process::Command::new("hyprctl")
            .arg("reload")
            .status()
            .map_err(|error| format!("cannot reload Hyprland: {error}"))?;
        if !status.success() {
            return Err("Hyprland rejected the profile reload".into());
        }
        let errors = std::process::Command::new("hyprctl")
            .arg("configerrors")
            .output()
            .map_err(|error| format!("cannot inspect Hyprland config errors: {error}"))?;
        if !errors.status.success() || !String::from_utf8_lossy(&errors.stdout).trim().is_empty() {
            return Err("Hyprland reported configuration errors".into());
        }
    }
    for wallpaper in &profile.wallpapers {
        let argument = format!(
            "{}, {}, {}",
            wallpaper.monitor,
            wallpaper.path.display(),
            wallpaper.fit.as_str()
        );
        let status = std::process::Command::new("hyprctl")
            .args(["hyprpaper", "wallpaper", &argument])
            .status()
            .map_err(|error| format!("cannot apply wallpaper: {error}"))?;
        if !status.success() {
            return Err("hyprpaper rejected the wallpaper".into());
        }
    }
    Ok(())
}

fn run_bars(
    command: BarsCommand,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    let paths = XdgPaths::from_environment().map_err(str::to_owned)?;
    match command {
        BarsCommand::Status => {
            let status = serde_json::json!({
                "waybar": waybar::detect(&paths.config_home),
                "quickshell": quickshell::detect(&paths.config_home),
            });
            write_value(format, output, &status, || {
                format!(
                    "Waybar: {:?}\nQuickshell: {:?}\n",
                    waybar::detect(&paths.config_home),
                    quickshell::detect(&paths.config_home)
                )
            })
        }
        BarsCommand::ApplyWaybar => {
            let theme = theme::builtins().remove(0);
            let plan = waybar::plan(&paths.config_home, &BarLayout::default(), &theme)?;
            let config_path = plan.config_path.clone();
            let transaction = TransactionPlan {
                summary: "Create managed Waybar configuration".into(),
                changes: vec![
                    write_change(&plan.config_path, plan.config.into_bytes())?,
                    write_change(&plan.style_path, plan.style.into_bytes())?,
                ],
            };
            let result = default_engine()?
                .apply(&transaction, || validate_managed_waybar(&config_path))
                .map_err(|error| error.to_string())?;
            write_value(format, output, &result, || {
                format!(
                    "Created managed Waybar config as transaction {}\nRun: waybar -c {} -s {}\n",
                    result.id,
                    config_path.display(),
                    config_path.with_file_name("style.css").display()
                )
            })
        }
        BarsCommand::ApplyQuickshell => {
            let theme = theme::builtins().remove(0);
            let plan = quickshell::plan(&paths.config_home, &BarLayout::default(), &theme)?;
            let shell_path = plan.shell_path.clone();
            let expected = plan.shell.clone();
            let transaction = TransactionPlan {
                summary: "Create cooperative Quickshell configuration".into(),
                changes: vec![write_change(&plan.shell_path, plan.shell.into_bytes())?],
            };
            let result = default_engine()?
                .apply(&transaction, || {
                    let actual = std::fs::read_to_string(&shell_path).map_err(|error| {
                        format!("cannot verify {}: {error}", shell_path.display())
                    })?;
                    (actual == expected)
                        .then_some(())
                        .ok_or_else(|| "generated Quickshell file changed during apply".into())
                })
                .map_err(|error| error.to_string())?;
            write_value(format, output, &result, || {
                format!(
                    "Created cooperative Quickshell config as transaction {}\nRun: quickshell --config helm-settings\n",
                    result.id
                )
            })
        }
    }
}

fn validate_managed_waybar(path: &std::path::Path) -> Result<(), String> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot verify {}: {error}", path.display()))?;
    let json = source.lines().skip(1).collect::<Vec<_>>().join("\n");
    serde_json::from_str::<serde_json::Value>(&json)
        .map(|_| ())
        .map_err(|error| format!("invalid managed Waybar JSONC: {error}"))
}

fn run_applications(
    command: ApplicationsCommand,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    let paths = XdgPaths::from_environment().map_err(str::to_owned)?;
    match command {
        ApplicationsCommand::Themes => {
            let themes = theme::builtins();
            write_value(format, output, &themes, || {
                themes.iter().fold(String::new(), |mut text, theme| {
                    writeln!(text, "{}\t{}", theme.id, theme.name)
                        .expect("writing to a String cannot fail");
                    text
                })
            })
        }
        ApplicationsCommand::AlacrittyStatus => {
            let status = alacritty::detect(&paths.config_home);
            write_value(format, output, &status, || format!("{status:?}\n"))
        }
        ApplicationsCommand::AlacrittyTheme { theme_id } => {
            let selected = find_theme(&theme_id)?;
            let root_path = paths.config_home.join("alacritty/alacritty.toml");
            let root = read_optional_text(&root_path)?;
            let plan = alacritty::plan_theme(&paths.config_home, &root, &selected)
                .map_err(|error| error.to_string())?;
            let transaction = TransactionPlan {
                summary: format!("Set Alacritty theme to {}", selected.name),
                changes: vec![
                    write_change(&plan.root_path, plan.root_content.into_bytes())?,
                    write_change(&plan.fragment_path, plan.fragment_content.into_bytes())?,
                ],
            };
            let root_path = plan.root_path;
            let fragment_path = plan.fragment_path;
            let result = default_engine()?
                .apply(&transaction, || {
                    validate_toml_file(&root_path)?;
                    validate_toml_file(&fragment_path)
                })
                .map_err(|error| error.to_string())?;
            write_value(format, output, &result, || {
                format!("Applied {} as transaction {}\n", selected.name, result.id)
            })
        }
        ApplicationsCommand::YaziFlavors => {
            let flavors =
                yazi::discover_flavors(&paths.config_home).map_err(|error| error.to_string())?;
            write_value(format, output, &flavors, || {
                flavors.iter().fold(String::new(), |mut text, flavor| {
                    writeln!(text, "{}\t{}", flavor.id, flavor.path.display())
                        .expect("writing to a String cannot fail");
                    text
                })
            })
        }
        ApplicationsCommand::YaziFlavor { flavor_id } => {
            let installed =
                yazi::discover_flavors(&paths.config_home).map_err(|error| error.to_string())?;
            if !installed.iter().any(|flavor| flavor.id == flavor_id) {
                return Err(format!("Yazi flavor `{flavor_id}` is not installed"));
            }
            let target = paths.config_home.join("yazi/theme.toml");
            let source = read_optional_text(&target)?;
            let replacement = yazi::select_flavor(&source, &flavor_id, &flavor_id)
                .map_err(|error| error.to_string())?;
            let transaction = TransactionPlan {
                summary: format!("Set Yazi flavor to {flavor_id}"),
                changes: vec![write_change(&target, replacement.into_bytes())?],
            };
            let verify_target = target.clone();
            let result = default_engine()?
                .apply(&transaction, || validate_toml_file(&verify_target))
                .map_err(|error| error.to_string())?;
            write_value(format, output, &result, || {
                format!("Applied {flavor_id} as transaction {}\n", result.id)
            })
        }
    }
}

fn find_theme(id: &str) -> Result<theme::Theme, String> {
    theme::builtins()
        .into_iter()
        .find(|theme| theme.id == id)
        .ok_or_else(|| format!("theme `{id}` was not found"))
}

fn read_optional_text(path: &std::path::Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

fn write_change(path: &std::path::Path, replacement: Vec<u8>) -> Result<FileChange, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let expected = match std::fs::read(path) {
        Ok(value) => Some(Fingerprint::bytes(&value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    Ok(FileChange::write(path, expected, replacement))
}

fn validate_toml_file(path: &std::path::Path) -> Result<(), String> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot verify {}: {error}", path.display()))?;
    source
        .parse::<toml_edit::DocumentMut>()
        .map(|_| ())
        .map_err(|error| format!("invalid TOML in {}: {error}", path.display()))
}

fn run_hyprland(
    command: HyprlandCommand,
    format: OutputFormat,
    output: &mut impl Write,
) -> Result<(), String> {
    let runtime = ProcessRuntime;
    match command {
        HyprlandCommand::Options => {
            let settings = runtime
                .descriptions()
                .map_err(|error| error.to_string())?
                .settings;
            write_value(format, output, &settings, || {
                format!("{} curated settings available\n", settings.len())
            })
        }
        HyprlandCommand::Displays => {
            let displays = runtime.displays().map_err(|error| error.to_string())?;
            write_value(format, output, &displays, || {
                displays.iter().fold(String::new(), |mut text, display| {
                    writeln!(
                        text,
                        "{}\t{}x{}@{:.2}\tscale {}",
                        display.name,
                        display.width,
                        display.height,
                        display.refresh_rate,
                        display.scale
                    )
                    .expect("writing to a String cannot fail");
                    text
                })
            })
        }
        HyprlandCommand::Bindings => {
            let bindings = runtime.bindings().map_err(|error| error.to_string())?;
            write_value(format, output, &bindings, || {
                format!("{} active bindings\n", bindings.len())
            })
        }
        HyprlandCommand::ConfigStatus => {
            let paths = XdgPaths::from_environment().map_err(str::to_owned)?;
            let generation = detect_generation(&paths.config_home.join("hypr"));
            write_value(format, output, &generation, || format!("{generation:?}\n"))
        }
    }
}

fn default_engine() -> Result<Engine, String> {
    let paths = XdgPaths::from_environment().map_err(str::to_owned)?;
    Engine::open(paths.helm_state(), paths.writable_roots()).map_err(|error| error.to_string())
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
