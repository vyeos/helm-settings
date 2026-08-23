#!/usr/bin/env bash
set -euo pipefail

reference=${1:-HEAD}
workspace=$(mktemp -d)
epoch=$(git show -s --format=%ct "$reference")

for build in one two; do
  source_dir="$workspace/source-$build"
  target_dir="$workspace/target-$build"
  mkdir -p "$source_dir"
  git archive "$reference" | tar -x -C "$source_dir"
  (
    cd "$source_dir"
    SOURCE_DATE_EPOCH="$epoch" \
      CARGO_TARGET_DIR="$target_dir" \
      RUSTFLAGS="--remap-path-prefix=$source_dir=/usr/src/helm-settings" \
      cargo build --frozen --release --bin helm-settings
  )
done

first="$workspace/target-one/release/helm-settings"
second="$workspace/target-two/release/helm-settings"
cmp --silent "$first" "$second"
sha256sum "$first" "$second"
printf 'reproducible release binaries retained in %s\n' "$workspace"
