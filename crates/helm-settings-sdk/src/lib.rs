//! Public SDK for out-of-process Helm plugins.

#![forbid(unsafe_code)]

use std::io::{self, BufReader};

pub use helm_plugin_protocol as protocol;

use protocol::{InitializeParams, InitializeResult, Request, Response, RpcError};

pub trait Plugin {
    fn initialize(&mut self, params: InitializeParams) -> Result<InitializeResult, RpcError>;

    fn inspect(&mut self, _params: serde_json::Value) -> Result<serde_json::Value, RpcError> {
        Err(method_not_found())
    }

    fn propose(&mut self, _params: serde_json::Value) -> Result<serde_json::Value, RpcError> {
        Err(method_not_found())
    }
}

pub fn serve(mut plugin: impl Plugin) -> io::Result<()> {
    let input = io::stdin();
    let mut reader = BufReader::new(input.lock());
    let output = io::stdout();
    let mut writer = output.lock();
    loop {
        let request: Request = protocol::read_message(&mut reader)?;
        let id = request.id.clone();
        let response = match dispatch(&mut plugin, request) {
            Ok(result) => Response {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            },
            Err(error) => Response {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(error),
            },
        };
        protocol::write_message(&mut writer, &response)?;
        if response
            .result
            .as_ref()
            .is_some_and(|value| value == "shutdown")
        {
            return Ok(());
        }
    }
}

fn dispatch(plugin: &mut impl Plugin, request: Request) -> Result<serde_json::Value, RpcError> {
    if request.jsonrpc != "2.0" {
        return Err(RpcError {
            code: -32_600,
            message: "invalid JSON-RPC version".into(),
        });
    }
    match request.method.as_str() {
        "initialize" => {
            let params =
                serde_json::from_value(request.params).map_err(|error| invalid_params(&error))?;
            let result = plugin.initialize(params)?;
            serde_json::to_value(result).map_err(|error| internal_error(&error))
        }
        "settings.inspect" => plugin.inspect(request.params),
        "settings.propose" => plugin.propose(request.params),
        "shutdown" => Ok(serde_json::Value::String("shutdown".into())),
        _ => Err(method_not_found()),
    }
}

fn method_not_found() -> RpcError {
    RpcError {
        code: -32_601,
        message: "method not found".into(),
    }
}

fn invalid_params(error: &serde_json::Error) -> RpcError {
    RpcError {
        code: -32_602,
        message: format!("invalid params: {error}"),
    }
}

fn internal_error(error: &serde_json::Error) -> RpcError {
    RpcError {
        code: -32_603,
        message: format!("internal serialization error: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Example;

    impl Plugin for Example {
        fn initialize(&mut self, _: InitializeParams) -> Result<InitializeResult, RpcError> {
            Ok(InitializeResult {
                protocol_version: protocol::PROTOCOL_VERSION.into(),
                plugin_name: "Example".into(),
                capabilities: protocol::Capabilities::default(),
                pages: Vec::new(),
            })
        }
    }

    #[test]
    fn initialize_dispatches_to_plugin() {
        let request = Request::new(
            protocol::RequestId::Number(1),
            "initialize",
            serde_json::json!({
                "protocol_version": protocol::PROTOCOL_VERSION,
                "host_version": "0.8.0",
                "locale": "en"
            }),
        );
        let value = dispatch(&mut Example, request).expect("initialize");
        assert_eq!(value["plugin_name"], "Example");
    }
}
