#!/usr/bin/env bash
# Build the ai-monitor menu bar app (single-file, no Xcode project).
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

command -v swiftc >/dev/null || {
    echo "swiftc not found. Install the Command Line Tools:" >&2
    echo "  xcode-select --install" >&2
    exit 1
}

echo "› Building (swiftc -O -parse-as-library)…"
swiftc -O -parse-as-library "$DIR/ai-monitor-menubar.swift" -o "$DIR/ai-monitor-menubar"
echo "✓ Built: $DIR/ai-monitor-menubar"
echo
echo "Rodar agora:        $DIR/ai-monitor-menubar &"
echo "Subir no login:     $DIR/install-agent.sh"
