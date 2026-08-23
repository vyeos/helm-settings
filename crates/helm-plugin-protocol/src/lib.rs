//! Versioned JSON-RPC contract for isolated Helm plugins.

#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "helm-plugin/1.0";
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl Request {
    #[must_use]
    pub fn new(id: RequestId, method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub host_version: String,
    pub locale: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub plugin_name: String,
    pub capabilities: Capabilities,
    #[serde(default)]
    pub pages: Vec<Page>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub inspect: bool,
    #[serde(default)]
    pub propose: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub controls: Vec<Control>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Control {
    Toggle {
        setting: String,
        label: String,
    },
    Text {
        setting: String,
        label: String,
    },
    Choice {
        setting: String,
        label: String,
        choices: Vec<String>,
    },
}

pub fn validate_initialize(result: &InitializeResult) -> Result<(), &'static str> {
    if result.protocol_version != PROTOCOL_VERSION {
        return Err("plugin protocol version is incompatible");
    }
    if result.plugin_name.is_empty() || result.plugin_name.len() > 128 || result.pages.len() > 32 {
        return Err("plugin metadata exceeds limits");
    }
    for page in &result.pages {
        if !valid_id(&page.id) || page.title.len() > 128 || page.groups.len() > 32 {
            return Err("invalid plugin page");
        }
        for group in &page.groups {
            if !valid_id(&group.id) || group.title.len() > 128 || group.controls.len() > 128 {
                return Err("invalid plugin group");
            }
        }
    }
    Ok(())
}

pub fn write_message(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let body = serde_json::to_vec(value).map_err(io::Error::other)?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message is too large",
        ));
    }
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

pub fn read_message<T: for<'de> Deserialize<'de>>(reader: &mut impl BufRead) -> io::Result<T> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "plugin closed stdout",
            ));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid length"))?,
            );
        }
    }
    let length =
        length.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing length"))?;
    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message is too large",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(io::Error::other)
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

    #[test]
    fn framed_message_round_trips() {
        let request = Request::new(RequestId::Number(1), "initialize", serde_json::json!({}));
        let mut bytes = Vec::new();
        write_message(&mut bytes, &request).expect("write");
        let decoded: Request = read_message(&mut bytes.as_slice()).expect("read");
        assert_eq!(decoded, request);
    }

    #[test]
    fn rejects_oversized_frame_before_allocating_body() {
        let source = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        assert!(read_message::<Request>(&mut source.as_bytes()).is_err());
    }
}
