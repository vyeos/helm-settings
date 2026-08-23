#![forbid(unsafe_code)]

fn main() {
    let first = std::env::args().nth(1);
    let graphical =
        std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();
    if first.as_deref() == Some("gui") || (first.is_none() && graphical) {
        helm_gui::run();
    } else if let Err(error) = helm_cli::run() {
        eprintln!("helm-settings: {error}");
        std::process::exit(2);
    }
}
