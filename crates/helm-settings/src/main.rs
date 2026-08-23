#![forbid(unsafe_code)]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("gui") {
        helm_gui::run();
    } else {
        helm_cli::run();
    }
}
