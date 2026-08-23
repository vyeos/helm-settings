#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut cursor = data;
    let _ = helm_plugin_protocol::read_message::<serde_json::Value>(&mut cursor);
});
