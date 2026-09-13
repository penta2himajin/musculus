#!/usr/bin/env bash
#
# Download the Irodori-TTS ONNX artifact bundle (M4 comparison candidate)
# into vendor/irodori/. The artifacts are the OFFICIAL WebGPU export set
# published by the irodori-tts-webgpu author (noguchis), bit-faithful
# against the official PyTorch runtime (corr = 1.000000, see
# .irodori-reference/README.md). musculus ships no weights.
#
# Licenses (see docs/model-licenses.md §4 and the artifact LICENSES/):
#   - Irodori-TTS-500M-v3 weights: MIT (no-impersonation terms on the model card)
#   - Semantic-DACVAE codec: MIT (derived from facebook/dacvae-watermarked)
#   - llm-jp-3-150m tokenizer: Apache-2.0
#
# Idempotent: skips files that already exist.
#
# Usage: scripts/setup_irodori.sh [dest-dir]   (default: vendor/irodori)

set -euo pipefail

DEST="${1:-vendor/irodori}"
BASE="https://huggingface.co/noguchis/irodori-tts-onnx/resolve/main"

mkdir -p "$DEST/onnx" "$DEST/tokenizer/llmjp_tok" "$DEST/LICENSES"

fetch() {
    local rel="$1"
    if [[ -s "$DEST/$rel" ]]; then
        echo "skip: $rel (present)"
        return
    fi
    echo "fetch: $rel"
    curl -fL --retry 3 --progress-bar -o "$DEST/$rel.part" "$BASE/$rel"
    mv "$DEST/$rel.part" "$DEST/$rel"
}

# Small files first, big .data files last (diagnose partial failures).
for f in LICENSES/Irodori-TTS-LICENSE LICENSES/Semantic-DACVAE-LICENSE \
         LICENSES/llm-jp-3-150m-LICENSE LICENSES/NOTICE \
         onnx/duration.onnx onnx/text_encoder.onnx onnx/speaker_encoder.onnx \
         onnx/dacvae_encoder.onnx onnx/dacvae_decoder.onnx onnx/dit.onnx \
         onnx/duration.onnx.data onnx/text_encoder.onnx.data \
         onnx/speaker_encoder.onnx.data onnx/dacvae_encoder.onnx.data \
         onnx/dacvae_decoder.onnx.data onnx/dit.onnx.data \
         tokenizer/llmjp_tok/tokenizer.json \
         tokenizer/llmjp_tok/tokenizer_config.json \
         tokenizer/llmjp_tok/special_tokens_map.json; do
    fetch "$f"
done

echo
echo "done: $DEST"
echo "license reminders:"
echo "  Irodori weights: MIT (no-impersonation terms; see LICENSES/Irodori-TTS-LICENSE)"
echo "  codec: MIT; tokenizer: Apache-2.0"