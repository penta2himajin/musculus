#!/usr/bin/env bash
# Fetch the SBV2 (JP-Extra) ONNX model bundle into vendor/sbv2/.
#
# musculus ships no weights (AGENTS.md, Prohibitions #1): this script
# downloads from the upstream distribution point and never rehosts.
# Idempotent — files already present and non-empty are skipped.
#
# License reminders for the fetched set (full details:
# docs/model-licenses.md):
#   - tsukuyomi voice: Tsukuyomi-chan character license — commercial
#     use allowed, prior contact not required, CREDIT REQUIRED.
#     https://tyc.rei-yumesaki.net/about/terms/
#   - deberta.onnx: ku-nlp deberta-v2 lineage, CC-BY-SA-4.0 (runtime
#     use fine; redistribution carries ShareAlike obligations — we
#     never redistribute).
#   - .sbv2 bundle contents derive from AGPL-3.0 training lineage;
#     loading at runtime does not impose it on this repo.
#   - tokenizer.json / ONNX Runtime binaries: MIT.
#
# Usage: scripts/setup_sbv2.sh [dest-dir]   (default: vendor/sbv2)

set -euo pipefail

DEST="${1:-vendor/sbv2}"
BASE="https://huggingface.co/googlefan/sbv2_onnx_models/resolve/main"

mkdir -p "$DEST"

fetch() {
    local name="$1"
    if [[ -s "$DEST/$name" ]]; then
        echo "skip: $DEST/$name (present)"
        return
    fi
    echo "fetch: $BASE/$name"
    curl -fL --retry 3 --progress-bar -o "$DEST/$name.part" "$BASE/$name"
    mv "$DEST/$name.part" "$DEST/$name"
}

fetch tokenizer.json
fetch deberta.onnx
fetch tsukuyomi.sbv2

echo
echo "done: $DEST"
echo "voice credit reminder (Tsukuyomi-chan character license):"
echo "  声: つくよみちゃん(CV. 夢前黎) — https://tyc.rei-yumesaki.net/"