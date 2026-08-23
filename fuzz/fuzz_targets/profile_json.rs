#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(profile) = serde_json::from_slice::<helm_adapter_desktop::profile::Profile>(data) {
        let _ = profile.validate();
    }
});
