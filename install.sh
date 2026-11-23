#!/usr/bin/env bash
# Zero-config one-command installer & builder for the complete zcstream / RUDP video streaming suite.
# Automatically clones or updates the 5 sibling repositories (zcrudp, zcvid, zpsync, zcstream, zcstream-app),
# builds all C/C++ targets and the Rust zcstream-app GUI, runs tests, and stages everything into ./dist/.
#
# Usage:
#   ./install.sh
#   ./install.sh --skip-tests
#   ./install.sh --workspace /path/to/RUDP

set -euo pipefail

WORKSPACE_DIR=""
SKIP_TESTS=0
NO_PULL=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --workspace)
      WORKSPACE_DIR="$2"
      shift 2
      ;;
    --skip-tests)
      SKIP_TESTS=1
      shift
      ;;
    --no-pull)
      NO_PULL=1
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: $0 [--workspace DIR] [--skip-tests] [--no-pull]" >&2
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -z "$WORKSPACE_DIR" ]]; then
  LEAF="$(basename "$SCRIPT_DIR")"
  case "$LEAF" in
    zcstream-app|zcstream|zcrudp|zcvid|zpsync|scripts)
      WORKSPACE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
      ;;
    *)
      WORKSPACE_DIR="$SCRIPT_DIR"
      ;;
  esac
fi

mkdir -p "$WORKSPACE_DIR"
ROOT="$(cd "$WORKSPACE_DIR" && pwd)"
JOBS="$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"

echo "=== zcstream / RUDP Unified Installer ==="
echo "Workspace : $ROOT"

if ! command -v git >/dev/null 2>&1; then
  echo "Error: git is required but not found in PATH." >&2
  exit 1
fi
if ! command -v cmake >/dev/null 2>&1; then
  echo "Error: cmake (>= 3.20) is required but not found in PATH." >&2
  exit 1
fi

REPOS=(
  "zcrudp:https://github.com/i-Charlys/zcrudp.git"
  "zcvid:https://github.com/i-Charlys/zcvid.git"
  "zpsync:https://github.com/i-Charlys/zpsync.git"
  "zcstream:https://github.com/i-Charlys/zcstream.git"
  "zcstream-app:https://github.com/i-Charlys/zcstream-app.git"
)

for entry in "${REPOS[@]}"; do
  NAME="${entry%%:*}"
  URL="${entry#*:}"
  REPO_PATH="$ROOT/$NAME"
  if [[ ! -d "$REPO_PATH/.git" ]]; then
    echo "[git] Cloning $NAME from $URL..."
    git clone "$URL" "$REPO_PATH"
  elif [[ "$NO_PULL" -eq 0 ]]; then
    if [[ -z "$(git -C "$REPO_PATH" status --porcelain)" ]]; then
      echo "[git] Updating $NAME..."
      git -C "$REPO_PATH" pull --ff-only >/dev/null 2>&1 || true
    else
      echo "[git] $NAME has local changes, keeping working tree as-is."
    fi
  fi
done

build_cmake_repo() {
  local name="$1"
  local src_dir="$2"
  shift 2
  local build_dir="$src_dir/build"
  echo ""
  echo "=== Building $name ==="
  cmake -S "$src_dir" -B "$build_dir" -DCMAKE_BUILD_TYPE=Release "$@"
  cmake --build "$build_dir" --config Release -j "$JOBS"
  if [[ "$SKIP_TESTS" -eq 0 ]]; then
    ctest --test-dir "$build_dir" --output-on-failure
  fi
}

build_cmake_repo "zcrudp"   "$ROOT/zcrudp"
build_cmake_repo "zcvid"    "$ROOT/zcvid"
build_cmake_repo "zpsync"   "$ROOT/zpsync"   "-DZCRUDP_DIR=$ROOT/zcrudp"
build_cmake_repo "zcstream" "$ROOT/zcstream" "-DZCRUDP_DIR=$ROOT/zcrudp" "-DZCVID_DIR=$ROOT/zcvid" "-DZPSYNC_DIR=$ROOT/zpsync"

DIST_DIR="$ROOT/dist"
mkdir -p "$DIST_DIR"
cmake --install "$ROOT/zcstream/build" --config Release --prefix "$DIST_DIR"

if command -v cargo >/dev/null 2>&1; then
  echo ""
  echo "=== Building zcstream-app (GUI Control Center) ==="
  (
    cd "$ROOT/zcstream-app"
    cargo build --release
    if [[ "$SKIP_TESTS" -eq 0 ]]; then
      cargo test --release
    fi
    if [[ -f "target/release/zcstream-app" ]]; then
      cp -f "target/release/zcstream-app" "$DIST_DIR/zcstream-app"
    fi
  )
else
  echo "[warn] cargo (Rust) not found in PATH — skipping zcstream-app GUI build."
fi

echo ""
echo "=== Installation Complete ==="
echo "All binaries are staged in: $DIST_DIR"
ls -1 "$DIST_DIR"
