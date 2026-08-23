#![forbid(unsafe_code)]

use std::{fs, path::Path, process::Command};

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_helm-settings"));
    command
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"));
    command
}

#[test]
fn cli_help_and_version_are_successful() {
    let home = tempfile::tempdir().expect("temporary home");
    for argument in ["--help", "--version"] {
        let output = command(home.path())
            .arg(argument)
            .output()
            .expect("run Helm");
        assert!(output.status.success(), "{argument} must be successful");
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn profile_apply_and_undo_are_atomic_from_a_clean_home() {
    let home = tempfile::tempdir().expect("temporary home");
    let profile = home.path().join("work.json");
    fs::write(
        &profile,
        r#"{
          "name":"Acceptance",
          "defaults":{"text/plain":"org.example.Editor.desktop"},
          "startup":[{"id":"org.example.Agent","name":"Agent","exec":"agent --quiet","enabled":false}]
        }"#,
    )
    .expect("profile fixture");

    let applied = command(home.path())
        .args(["--output", "json", "profiles", "apply"])
        .arg(&profile)
        .output()
        .expect("apply profile");
    assert!(
        applied.status.success(),
        "profile apply failed: {}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&applied.stdout).expect("JSON apply envelope");
    assert_eq!(envelope["ok"], true);
    let transaction = envelope["data"]["id"].as_str().expect("transaction id");
    let mimeapps = home.path().join("config/mimeapps.list");
    let startup = home
        .path()
        .join("config/autostart/org.example.Agent.desktop");
    assert!(
        fs::read_to_string(&mimeapps)
            .expect("mimeapps")
            .contains("text/plain=org.example.Editor.desktop;")
    );
    assert!(
        fs::read_to_string(&startup)
            .expect("startup entry")
            .contains("Hidden=true")
    );

    let undone = command(home.path())
        .args(["history", "undo", transaction])
        .output()
        .expect("undo profile");
    assert!(
        undone.status.success(),
        "undo failed: {}",
        String::from_utf8_lossy(&undone.stderr)
    );
    assert!(!mimeapps.exists());
    assert!(!startup.exists());

    let history = command(home.path())
        .args(["--output", "json", "history", "list"])
        .output()
        .expect("read history");
    let envelope: serde_json::Value =
        serde_json::from_slice(&history.stdout).expect("JSON history envelope");
    assert_eq!(envelope["data"].as_array().map(Vec::len), Some(2));
}

#[test]
fn rejected_profile_and_legacy_config_are_never_modified() {
    let home = tempfile::tempdir().expect("temporary home");
    let hypr = home.path().join("config/hypr");
    fs::create_dir_all(&hypr).expect("Hyprland directory");
    let legacy = hypr.join("hyprland.conf");
    let original = b"# owned by the user\ngeneral { gaps_in = 4 }\n";
    fs::write(&legacy, original).expect("legacy fixture");
    let invalid = home.path().join("invalid.json");
    fs::write(&invalid, r#"{"name":"","startup":[]}"#).expect("invalid fixture");

    let status = command(home.path())
        .args(["hyprland", "config-status"])
        .output()
        .expect("inspect legacy config");
    assert!(status.status.success());
    assert_eq!(String::from_utf8_lossy(&status.stdout), "LegacyReadOnly\n");

    let rejected = command(home.path())
        .args(["profiles", "apply"])
        .arg(&invalid)
        .output()
        .expect("reject profile");
    assert!(!rejected.status.success());
    assert_eq!(fs::read(&legacy).expect("legacy remains"), original);
    assert!(!home.path().join("state/helm-settings").exists());
}
