#!/usr/bin/env bash
# Regenerate every packaged app icon from the code-drawn artwork (crates/ui-egui/src/brand/icon.rs).
# The outputs are committed, so builds and packaging never need this.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo run --release -q -p gridcraft-ui-egui --example app_icon -- "$ROOT/assets/app-icon"
