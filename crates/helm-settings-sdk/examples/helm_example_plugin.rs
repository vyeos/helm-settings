use helm_settings_sdk::{Plugin, protocol, serve};

struct ExamplePlugin;

impl Plugin for ExamplePlugin {
    fn initialize(
        &mut self,
        _params: protocol::InitializeParams,
    ) -> Result<protocol::InitializeResult, protocol::RpcError> {
        Ok(protocol::InitializeResult {
            protocol_version: protocol::PROTOCOL_VERSION.into(),
            plugin_name: "Helm SDK Example".into(),
            capabilities: protocol::Capabilities {
                inspect: true,
                propose: false,
            },
            pages: vec![protocol::Page {
                id: "example".into(),
                title: "Example".into(),
                groups: vec![protocol::Group {
                    id: "behavior".into(),
                    title: "Behavior".into(),
                    controls: vec![protocol::Control::Toggle {
                        setting: "example.enabled".into(),
                        label: "Enabled".into(),
                    }],
                }],
            }],
        })
    }
}

fn main() -> std::io::Result<()> {
    serve(ExamplePlugin)
}
