//! Discovery and fail-closed Bubblewrap hosting for third-party plugins.

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use helm_plugin_protocol::{
    InitializeParams, InitializeResult, PROTOCOL_VERSION, Request, RequestId, Response,
    validate_initialize,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("invalid plugin: {0}")]
    Invalid(String),
    #[error("plugin sandbox is unavailable: {0}")]
    Sandbox(String),
    #[error("plugin I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("plugin timed out")]
    Timeout,
    #[error("plugin protocol failed: {0}")]
    Protocol(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub developer_unsigned: bool,
}

impl Manifest {
    pub fn validate(&self) -> Result<(), HostError> {
        if self.schema_version != 1 {
            return Err(HostError::Invalid("unsupported manifest version".into()));
        }
        if !valid_id(&self.id) || self.name.is_empty() || self.name.len() > 128 {
            return Err(HostError::Invalid("invalid plugin identity".into()));
        }
        let executable = Path::new(&self.executable);
        if executable.components().count() != 1 || self.executable.starts_with('.') {
            return Err(HostError::Invalid(
                "plugin executable must be one relative filename".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstalledPlugin {
    pub manifest: Manifest,
    pub directory: PathBuf,
}

impl InstalledPlugin {
    pub fn load(directory: &Path, developer_mode: bool) -> Result<Self, HostError> {
        let directory = fs::canonicalize(directory)?;
        let source = fs::read(directory.join("plugin.json"))?;
        let manifest: Manifest = serde_json::from_slice(&source)
            .map_err(|error| HostError::Invalid(format!("invalid plugin.json: {error}")))?;
        manifest.validate()?;
        if manifest.developer_unsigned && !developer_mode {
            return Err(HostError::Invalid(
                "unsigned plugin requires developer mode".into(),
            ));
        }
        let executable = directory.join(&manifest.executable);
        if !executable.is_file() {
            return Err(HostError::Invalid("plugin executable is missing".into()));
        }
        Ok(Self {
            manifest,
            directory,
        })
    }
}

#[must_use]
pub fn discover(root: &Path, developer_mode: bool) -> Vec<Result<InstalledPlugin, HostError>> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut directories = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories
        .iter()
        .map(|directory| InstalledPlugin::load(directory, developer_mode))
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SandboxSpec {
    pub program: String,
    pub arguments: Vec<String>,
}

pub fn sandbox_spec(plugin: &InstalledPlugin) -> Result<SandboxSpec, HostError> {
    if !command_exists("bwrap") {
        return Err(HostError::Sandbox("Bubblewrap is not installed".into()));
    }
    let directory = plugin
        .directory
        .to_str()
        .ok_or_else(|| HostError::Invalid("plugin path is not UTF-8".into()))?;
    let executable = format!("/plugin/{}", plugin.manifest.executable);
    Ok(SandboxSpec {
        program: "bwrap".into(),
        arguments: vec![
            "--die-with-parent".into(),
            "--new-session".into(),
            "--unshare-all".into(),
            "--clearenv".into(),
            "--ro-bind".into(),
            "/usr".into(),
            "/usr".into(),
            "--ro-bind".into(),
            "/etc".into(),
            "/etc".into(),
            "--symlink".into(),
            "usr/bin".into(),
            "/bin".into(),
            "--symlink".into(),
            "usr/lib".into(),
            "/lib".into(),
            "--symlink".into(),
            "usr/lib64".into(),
            "/lib64".into(),
            "--proc".into(),
            "/proc".into(),
            "--dev".into(),
            "/dev".into(),
            "--tmpfs".into(),
            "/tmp".into(),
            "--ro-bind".into(),
            directory.into(),
            "/plugin".into(),
            "--chdir".into(),
            "/plugin".into(),
            "--setenv".into(),
            "PATH".into(),
            "/usr/bin:/bin".into(),
            "--setenv".into(),
            "LANG".into(),
            "C.UTF-8".into(),
            "--".into(),
            executable,
        ],
    })
}

pub fn probe(
    plugin: &InstalledPlugin,
    host_version: &str,
    timeout: Duration,
) -> Result<InitializeResult, HostError> {
    let spec = sandbox_spec(plugin)?;
    let mut child = Command::new(&spec.program)
        .args(&spec.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| HostError::Protocol("plugin stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HostError::Protocol("plugin stdout unavailable".into()))?;
    let request = Request::new(
        RequestId::Number(1),
        "initialize",
        serde_json::to_value(InitializeParams {
            protocol_version: PROTOCOL_VERSION.into(),
            host_version: host_version.into(),
            locale: "en".into(),
        })
        .map_err(|error| HostError::Protocol(error.to_string()))?,
    );
    helm_plugin_protocol::write_message(&mut stdin, &request)?;
    stdin.flush()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let response = helm_plugin_protocol::read_message::<Response>(&mut BufReader::new(stdout));
        let _ = sender.send(response);
    });
    let response = match receiver.recv_timeout(timeout) {
        Ok(response) => response?,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(HostError::Timeout);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err(HostError::Protocol("plugin reader stopped".into()));
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    if response.jsonrpc != "2.0" || response.id != RequestId::Number(1) {
        return Err(HostError::Protocol("invalid initialize response".into()));
    }
    if let Some(error) = response.error {
        return Err(HostError::Protocol(error.message));
    }
    let result: InitializeResult = serde_json::from_value(
        response
            .result
            .ok_or_else(|| HostError::Protocol("initialize result is missing".into()))?,
    )
    .map_err(|error| HostError::Protocol(error.to_string()))?;
    validate_initialize(&result).map_err(|error| HostError::Protocol(error.into()))?;
    Ok(result)
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| directory.join(command).is_file())
    })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn unsigned_plugin_fails_closed_without_developer_mode() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(
            root.path().join("plugin.json"),
            r#"{"schema_version":1,"id":"example","name":"Example","executable":"plugin","developer_unsigned":true}"#,
        )
        .expect("manifest");
        fs::write(root.path().join("plugin"), "#!/bin/sh\n").expect("executable");
        fs::set_permissions(
            root.path().join("plugin"),
            fs::Permissions::from_mode(0o700),
        )
        .expect("permissions");
        assert!(InstalledPlugin::load(root.path(), false).is_err());
        assert!(InstalledPlugin::load(root.path(), true).is_ok());
    }

    #[test]
    fn sandbox_has_no_network_or_home_bind() {
        if !command_exists("bwrap") {
            return;
        }
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(
            root.path().join("plugin.json"),
            r#"{"schema_version":1,"id":"example","name":"Example","executable":"plugin"}"#,
        )
        .expect("manifest");
        fs::write(root.path().join("plugin"), "binary").expect("executable");
        let plugin = InstalledPlugin::load(root.path(), false).expect("plugin");
        let spec = sandbox_spec(&plugin).expect("sandbox");
        assert!(spec.arguments.contains(&"--unshare-all".into()));
        assert!(
            !spec
                .arguments
                .iter()
                .any(|argument| argument.contains("/home"))
        );
    }
}
