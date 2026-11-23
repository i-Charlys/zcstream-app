#!/usr/bin/env bash
# Zero-config one-command installer & builder for the complete zcstream / RUDP video streaming suite.
# Automatically clones or downloads the 5 sibling repositories (zcrudp, zcvid, zpsync, zcstream, zcstream-app),
# builds all C/C++ targets and the Rust zcstream-app GUI, runs tests, and stages everything into ./dist/.
#
# Works whether invoked from a git clone, a GitHub "Download ZIP" folder (<repo>-main), or an empty folder,
# and falls back to GitHub ZIP/tar.gz archives automatically if git is not installed.
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
  if [[ "$LEAF" =~ ^(zcstream-app|zcstream|zcrudp|zcvid|zpsync|scripts)(-[A-Za-z0-9._-]+)?$ ]] || \
     [[ -f "$SCRIPT_DIR/Cargo.toml" && ! -d "$SCRIPT_DIR/zcstream" ]] || \
     [[ -f "$SCRIPT_DIR/src/stream_live.c" && ! -d "$SCRIPT_DIR/zcstream" ]]; then
    WORKSPACE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
  else
    WORKSPACE_DIR="$SCRIPT_DIR"
  fi
fi

mkdir -p "$WORKSPACE_DIR"
ROOT="$(cd "$WORKSPACE_DIR" && pwd)"
JOBS="$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"

echo "=== zcstream / RUDP Unified Installer ==="
echo "Workspace : $ROOT"

HAS_GIT=0
if command -v git >/dev/null 2>&1; then
  HAS_GIT=1
fi
if ! command -v cmake >/dev/null 2>&1; then
  echo "Error: cmake (>= 3.21) is required but not found in PATH." >&2
  exit 1
fi

download_repo_archive() {
  local name="$1"
  local git_url="$2"
  local dest="$3"
  local check_rel="$4"
  local base_url="${git_url%.git}"
  local tar_url="$base_url/archive/refs/heads/main.tar.gz"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  echo "[archive] Downloading $name from $tar_url..."
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$tar_url" | tar -xz -C "$tmp_dir"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$tar_url" | tar -xz -C "$tmp_dir"
  else
    rm -rf "$tmp_dir"
    echo "Error: neither git, curl, nor wget found to download $name." >&2
    exit 1
  fi
  local extracted
  extracted="$(find "$tmp_dir" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
  if [[ -z "$extracted" || ! -f "$extracted/$check_rel" ]]; then
    rm -rf "$tmp_dir"
    echo "Error: archive for $name missing $check_rel" >&2
    exit 1
  fi
  rm -rf "$dest"
  mv "$extracted" "$dest"
  rm -rf "$tmp_dir"
}

resolve_or_fetch_repo() {
  local name="$1"
  local url="$2"
  local check_rel="$3"
  local found=""

  for cand in "$ROOT/$name" "$ROOT/$name-main" "$ROOT/$name-master" "$SCRIPT_DIR"; do
    if [[ -f "$cand/$check_rel" ]]; then
      found="$(cd "$cand" && pwd)"
      break
    fi
  done

  if [[ -n "$found" ]]; then
    if [[ "$HAS_GIT" -eq 1 && -d "$found/.git" ]]; then
      if [[ "$NO_PULL" -eq 0 ]]; then
        if [[ -z "$(git -C "$found" status --porcelain)" ]]; then
          echo "[git] Updating $name..." >&2
          git -C "$found" pull --ff-only >/dev/null 2>&1 || true
        else
          echo "[git] $name has local changes, keeping working tree as-is." >&2
        fi
      fi
    else
      echo "[repo] Using existing $name at $found" >&2
    fi
    echo "$found"
    return
  fi

  local dest="$ROOT/$name"
  local cloned=0
  if [[ "$HAS_GIT" -eq 1 ]]; then
    echo "[git] Cloning $name from $url..." >&2
    rm -rf "$dest"
    if git clone "$url" "$dest" >&2 && [[ -f "$dest/$check_rel" ]]; then
      cloned=1
    fi
  fi
  if [[ "$cloned" -eq 0 ]]; then
    download_repo_archive "$name" "$url" "$dest" "$check_rel" >&2
  fi
  echo "$dest"
}

ZCRUDP_DIR="$(resolve_or_fetch_repo "zcrudp" "https://github.com/i-Charlys/zcrudp.git" "include/rudp_nextgen.h")"
ZCVID_DIR="$(resolve_or_fetch_repo "zcvid" "https://github.com/i-Charlys/zcvid.git" "include/zcvid_codec.h")"
ZPSYNC_DIR="$(resolve_or_fetch_repo "zpsync" "https://github.com/i-Charlys/zpsync.git" "include/zpsync.h")"
ZCSTREAM_DIR="$(resolve_or_fetch_repo "zcstream" "https://github.com/i-Charlys/zcstream.git" "src/stream_live.c")"
APP_DIR="$(resolve_or_fetch_repo "zcstream-app" "https://github.com/i-Charlys/zcstream-app.git" "Cargo.toml")"

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

build_cmake_repo "zcrudp"   "$ZCRUDP_DIR"
build_cmake_repo "zcvid"    "$ZCVID_DIR"
build_cmake_repo "zpsync"   "$ZPSYNC_DIR"   "-DZCRUDP_DIR=$ZCRUDP_DIR"
build_cmake_repo "zcstream" "$ZCSTREAM_DIR" "-DZCRUDP_DIR=$ZCRUDP_DIR" "-DZCVID_DIR=$ZCVID_DIR" "-DZPSYNC_DIR=$ZPSYNC_DIR"

DIST_DIR="$ROOT/dist"
mkdir -p "$DIST_DIR"
cmake --install "$ZCSTREAM_DIR/build" --config Release --prefix "$DIST_DIR"

if command -v cargo >/dev/null 2>&1; then
  echo ""
  echo "=== Building zcstream-app (GUI Control Center) ==="
  (
    cd "$APP_DIR"
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
