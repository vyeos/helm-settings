#![forbid(unsafe_code)]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("gui") {
        helm_gui::run();
    } else if let Err(error) = helm_cli::run() {
        eprintln!("helm-settings: {error}");
        std::process::exit(2);
    }
}
