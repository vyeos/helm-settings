#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo build --frozen --release --bin helm-settings
cargo check --manifest-path fuzz/Cargo.toml --locked
desktop-file-validate data/io.github.vyeos.HelmSettings.desktop
appstreamcli validate data/io.github.vyeos.HelmSettings.metainfo.xml
bash -n packaging/arch/PKGBUILD.in
scripts/check-html-links.py
