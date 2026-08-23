#![no_main]

use std::path::Path;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let theme = helm_adapter_applications::theme::builtins().remove(0);
        let _ =
            helm_adapter_applications::alacritty::plan_theme(Path::new("/config"), source, &theme);
        let _ = helm_adapter_applications::yazi::select_flavor(source, "dark", "light");
    }
});
