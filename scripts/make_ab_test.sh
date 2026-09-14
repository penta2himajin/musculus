#!/usr/bin/env bash
#
# Blind A/B sample generator: SBV2JE vs Irodori-TTS on identical texts.
#
# Writes ab-test/pair-NN/{A,B}.wav (the A/B→engine assignment is
# randomized per pair) and ab-test/key.ndjson — the mapping. key.ndjson
# is gitignored so the assignment stays blind until the listener has
# finished scoring; reveal it afterwards.
#
# Both engines receive the SAME raw text and each runs its own native
# preprocessing (SBV2: JaNormalizer + jpreprocess frontend; Irodori:
# its own normalize_text + llm-jp tokenizer). That is the honest
# whole-pipeline comparison, not an engine-core-only one.
#
# Both sides are then put in one comparable container: 48 kHz mono,
# -16 LUFS integrated loudness, peak-limited. Without this the container
# (44.1 vs 48 kHz) and the loudness (SBV2 peaks ~0.3-0.5 vs Irodori 1.0)
# would leak the engine and bias the preference judgement.
#
# Usage: scripts/make_ab_test.sh [out-dir]     (default: ab-test)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="${1:-ab-test}"
REF_WAV="${IRODORI_REF_WAV:-vendor/irodori-ref.wav}"
STEPS="${IRODORI_STEPS:-40}"
LUFS="${AB_LUFS:--16}"
CARGO_HOME="${CARGO_HOME:-$ROOT/.cargo-home}"
export CARGO_HOME

if [[ ! -f "$REF_WAV" ]]; then
    echo "[error] reference WAV not found: $REF_WAV" >&2
    echo "        run scripts/setup_irodori.sh, then generate one with:" >&2
    echo "        cargo run --release --features cli,onnx -- synth \"...\" --out vendor/irodori-ref.wav" >&2
    exit 3
fi

# Identical text for both engines. Numerals/dates/symbols are included on
# purpose: each engine's own normalizer handles them its own way.
texts=(
    "こんにちは。今日はとても良い天気ですね。"
    "2026年9月13日、午後3時から会議を始めます。"
    "その森には、古い言い伝えがありました。月が最も高く昇る夜、静かに耳を澄ませば、風の歌声が聞こえるというのです。"
    "この製品の成功率は、昨年より5パーセント上がりました。"
    "ご注文は、1,200円になります。ありがとうございました。"
)

rm -rf "$OUT"
mkdir -p "$OUT"
: > "$OUT/key.ndjson"

for i in "${!texts[@]}"; do
    text="${texts[$i]}"
    n="$(printf '%02d' $((i + 1)))"
    dir="$OUT/pair-$n"
    mkdir -p "$dir"

    echo "[pair-$n] sbv2 ..."
    if ! cargo run --release --features cli,onnx --quiet -- \
        synth "$text" --out "$dir/.sbv2.raw.wav" >"$dir/.sbv2.log" 2>&1; then
        echo "[error] sbv2 synthesis failed for pair-$n" >&2
        tail -5 "$dir/.sbv2.log" >&2
        exit 1
    fi

    echo "[pair-$n] irodori (steps=$STEPS) ..."
    if ! cargo run --release --features onnx,wav --quiet --example irodori_demo -- \
        --text "$text" --ref-wav "$REF_WAV" --steps "$STEPS" \
        --out "$dir/.irodori.raw.wav" >"$dir/.irodori.log" 2>&1; then
        echo "[error] irodori synthesis failed for pair-$n" >&2
        tail -5 "$dir/.irodori.log" >&2
        exit 1
    fi

    echo "[pair-$n] prep (48 kHz, $LUFS LUFS) ..."
    for engine in sbv2 irodori; do
        if ! cargo run --release --features onnx,wav --quiet --example prep_ab_audio -- \
            --input "$dir/.$engine.raw.wav" --out "$dir/.$engine.wav" \
            --rate 48000 --lufs="$LUFS" >"$dir/.$engine.prep.log" 2>&1; then
            echo "[error] prep failed for pair-$n ($engine)" >&2
            tail -5 "$dir/.$engine.prep.log" >&2
            exit 1
        fi
    done

    # Randomize which engine is A and which is B, per pair.
    if (( RANDOM % 2 )); then
        a=sbv2
        b=irodori
    else
        a=irodori
        b=sbv2
    fi
    mv "$dir/.$a.wav" "$dir/A.wav"
    mv "$dir/.$b.wav" "$dir/B.wav"
    rm -f "$dir"/.*.log "$dir"/.*.raw.wav

    printf '{"pair": "pair-%s", "text": %s, "A": "%s", "B": "%s"}\n' \
        "$n" \
        "$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1], ensure_ascii=False))' "$text")" \
        "$a" "$b" >> "$OUT/key.ndjson"
done

echo
echo "done: $OUT/"
echo "  pair-NN/A.wav  pair-NN/B.wav   A/B randomized per pair"
echo "  key.ndjson                     engine mapping (gitignored; open only after scoring)"
ls "$OUT"