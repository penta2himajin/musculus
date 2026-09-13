#!/usr/bin/env bash
#
# Download the round-trip ruler ASR: the ONNX-exported
# `nvidia/parakeet-tdt_ctc-0.6b-ja` bundle (~2.4 GB) from the
# `sunilmahendrakar/parakeet-tdt-0.6b-ja-onnx` HuggingFace mirror.
#
# This is the SAME model euhadra's L1 uses for ja (docs/evaluation.md
# in euhadra); the ruler is shared so CER numbers are comparable across
# the two projects. Script adapted from euhadra's
# scripts/setup_parakeet_ja.sh with permission-by-sameness (same
# author); keep the download layout identical to what
# `parakeet-rs::ParakeetTDT::from_pretrained` expects (encoder-model.onnx
# + .data, decoder_joint-model.onnx + .data, vocab.txt, config.json).
#
# Idempotent: skips files that already exist. Pass PARAKEET_JA_DIR to
# override the default location.
#
# Licensing (informational — defer to upstream URLs):
#   - nvidia/parakeet-tdt_ctc-0.6b-ja: CC-BY-4.0
#   - sunilmahendrakar/parakeet-tdt-0.6b-ja-onnx (the actual download):
#     CC-BY-4.0 (inherited)
#   Attribution required: credit NVIDIA, link the CC-BY-4.0 text.

set -euo pipefail

DIR="${PARAKEET_JA_DIR:-vendor/parakeet_ja}"
HF_REPO="https://huggingface.co/sunilmahendrakar/parakeet-tdt-0.6b-ja-onnx/resolve/main"

mkdir -p "$DIR"

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "[error] required tool '$1' not on PATH" >&2
        exit 3
    fi
}
require curl

# Order matters: large `.data` files are most likely to fail on flaky
# networks; placing them later means small headers + vocab succeed
# first, leaving an obvious diagnostic if the big chunks fail.
for f in vocab.txt config.json encoder-model.onnx decoder_joint-model.onnx encoder-model.onnx.data decoder_joint-model.onnx.data; do
    target="$DIR/$f"
    if [[ -s "$target" ]]; then
        echo "[skip] $f already present"
        continue
    fi
    echo "[get] $f"
    curl -fL --retry 3 --retry-delay 2 --max-time 1200 \
        -o "$target" "$HF_REPO/$f"
done

# Sanity check: the encoder + its external weights file must both
# exist. parakeet-rs will SIGSEGV if .data is missing.
if [[ ! -s "$DIR/encoder-model.onnx" ]] || [[ ! -s "$DIR/encoder-model.onnx.data" ]]; then
    echo "[error] $DIR is missing encoder-model.onnx and/or encoder-model.onnx.data" >&2
    exit 4
fi

echo "PARAKEET_JA_DIR=$DIR"