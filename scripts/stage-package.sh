#!/usr/bin/env bash
set -euo pipefail

binary=${1:-target/release/helm-settings}
destination=${2:?usage: scripts/stage-package.sh [binary] DESTDIR}

install -Dm755 "$binary" "$destination/usr/bin/helm-settings"
install -Dm644 data/io.github.vyeos.HelmSettings.desktop "$destination/usr/share/applications/io.github.vyeos.HelmSettings.desktop"
install -Dm644 data/io.github.vyeos.HelmSettings.metainfo.xml "$destination/usr/share/metainfo/io.github.vyeos.HelmSettings.metainfo.xml"
install -Dm644 data/icons/hicolor/scalable/apps/io.github.vyeos.HelmSettings.svg "$destination/usr/share/icons/hicolor/scalable/apps/io.github.vyeos.HelmSettings.svg"
install -Dm644 LICENSES/MPL-2.0.txt "$destination/usr/share/licenses/helm-settings/LICENSE"

desktop-file-validate "$destination/usr/share/applications/io.github.vyeos.HelmSettings.desktop"
appstreamcli validate "$destination/usr/share/metainfo/io.github.vyeos.HelmSettings.metainfo.xml"
"$destination/usr/bin/helm-settings" --version
find "$destination" -type f -printf '%P\n' | sort
