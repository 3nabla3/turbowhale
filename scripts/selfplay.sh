#!/usr/bin/env bash
# Usage: ./scripts/selfplay.sh <baseline_tag> [games] [tc]
#   baseline_tag: release version without leading "v" (e.g. 1.4.0)
#   games:        default 500 (total, split into rounds of 2 with color swap)
#   tc:           default "10+0.1"
set -euo pipefail

tag="${1:?need baseline tag, e.g. 1.4.0}"
games="${2:-500}"
tc="${3:-10+0.1}"

repo="3nabla3/turbowhale"

arch="$(uname -m)"
case "$arch" in
    x86_64|aarch64) ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
esac

case "$(uname -s)" in
    Linux)  os="linux"  ;;
    Darwin) os="macos"  ;;
    *) echo "Unsupported OS: $(uname -s) — script supports Linux and macOS" >&2; exit 1 ;;
esac

mkdir -p engines
baseline="engines/turbowhale-v${tag}"
if [[ ! -x "$baseline" ]]; then
    asset="turbowhale-v${tag}-${arch}-${os}"
    url="https://github.com/${repo}/releases/download/v${tag}/${asset}"
    echo "Downloading $asset from $url ..."
    if ! curl -fL -o "$baseline" "$url"; then
        echo "Failed to download $url — check that the release exists for this platform." >&2
        rm -f "$baseline"
        exit 1
    fi
    chmod +x "$baseline"
fi

echo "Building challenger from working tree ..."
cargo build --release
challenger="$(pwd)/target/release/turbowhale"

if ! command -v fastchess >/dev/null 2>&1; then
    echo "fastchess not found on PATH — install from https://github.com/Disservin/fastchess" >&2
    exit 1
fi

rounds=$(( games / 2 ))
concurrency="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)"

fastchess \
    -engine cmd="$challenger" name="dev" \
    -engine cmd="$baseline"   name="v${tag}" \
    -each tc="$tc" proto=uci \
    -rounds "$rounds" -games 2 -repeat \
    -openings file=scripts/openings.epd format=epd order=random \
    -sprt elo0=0 elo1=10 alpha=0.05 beta=0.05 \
    -concurrency "$concurrency" \
    -pgnout file=selfplay.pgn notation=san
