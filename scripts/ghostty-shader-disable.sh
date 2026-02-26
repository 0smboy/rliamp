#!/usr/bin/env bash
set -euo pipefail

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/ghostty"
CONFIG_FILE="$CONFIG_DIR/config"

BEGIN_MARK="# >>> RLIAMP-GHOSTTY-SHADERS >>>"
END_MARK="# <<< RLIAMP-GHOSTTY-SHADERS <<<"

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "Ghostty config not found: $CONFIG_FILE"
  exit 0
fi

tmp_file="$(mktemp)"
cleanup() {
  rm -f "$tmp_file"
}
trap cleanup EXIT

awk -v begin="$BEGIN_MARK" -v end="$END_MARK" '
  $0 == begin { in_block = 1; next }
  $0 == end   { in_block = 0; next }
  !in_block   { print }
' "$CONFIG_FILE" >"$tmp_file"

mv "$tmp_file" "$CONFIG_FILE"
echo "Removed RLIAMP shader block from: $CONFIG_FILE"
echo "Restart Ghostty to apply."
