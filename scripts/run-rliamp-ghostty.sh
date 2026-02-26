#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PROFILE="crisp"
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage:
  ./scripts/run-rliamp-ghostty.sh [--profile crisp|balanced|neon] [--dry-run] [rliamp args...]

Examples:
  ./scripts/run-rliamp-ghostty.sh /Users/me/Music/song.mp3
  ./scripts/run-rliamp-ghostty.sh --profile balanced /Users/me/Music
  ./scripts/run-rliamp-ghostty.sh --profile neon --dry-run /Users/me/Music/song.mp3
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      PROFILE="${2:-}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    *)
      break
      ;;
  esac
done

case "$PROFILE" in
  crisp|balanced|neon) ;;
  *)
    echo "Unknown profile: $PROFILE"
    echo "Supported profiles: crisp, balanced, neon"
    exit 1
    ;;
esac

# Ensure shaders are installed locally (no global config mutation).
"$SCRIPT_DIR/ghostty-shader-import.sh" --profile "$PROFILE" >/dev/null

SHADER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/ghostty/shaders/rliamp-scoped"
case "$PROFILE" in
  crisp)
    SHADERS=("sin-interference.glsl" "dither.glsl")
    ;;
  balanced)
    SHADERS=("crt.glsl" "sin-interference.glsl")
    ;;
  neon)
    SHADERS=("crt.glsl" "glow-rgbsplit-twitchy.glsl" "bloom.glsl" "sin-interference.glsl")
    ;;
esac

RLIAMP_BIN="${RLIAMP_BIN:-$ROOT_DIR/target-user/release/rliamp}"
if [[ ! -x "$RLIAMP_BIN" ]]; then
  (cd "$ROOT_DIR" && cargo build --release >/dev/null)
fi
if [[ ! -x "$RLIAMP_BIN" ]]; then
  echo "rliamp binary not found: $RLIAMP_BIN"
  exit 1
fi

shader_args=()
for shader in "${SHADERS[@]}"; do
  shader_args+=("--custom-shader=$SHADER_DIR/$shader")
done

if [[ "$(uname -s)" == "Darwin" ]]; then
  cmd=(open -na Ghostty.app --args "${shader_args[@]}" -e "$RLIAMP_BIN" "$@")
else
  cmd=(ghostty "${shader_args[@]}" -e "$RLIAMP_BIN" "$@")
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
  printf 'Command: '
  printf '%q ' "${cmd[@]}"
  printf '\n'
  exit 0
fi

"${cmd[@]}"
