#!/usr/bin/env bash
#
# Blind A/B: one engine, two step counts (low vs high) — the low-step
# quality test that ADR-0005 leaves open.
#
# Same engine, same seed, same text; only the number of rectified-flow
# Euler steps differs. Both sides are put in one comparable container:
# 48 kHz mono, normalized to a common target and peak-limited.
#
# The target is -20 LUFS, not -16: low-step diffusion output is peaky, so
# at -16 the peak limiter engages and leaves the two sides ~3.6 dB apart
# (measured: RMS 0.14 vs 0.09), which would leak the step count through
# loudness and bias the preference. At -20 both sides reach the target
# without limiting, and the script verifies the achieved loudness pair by
# pair before presenting anything.
#
# Writes listening/02-irodori-steps/pair-NN/{A,B}.wav and key.ndjson (gitignored,
# revealed only after scoring).
#
# Usage: scripts/make_ab_test_steps.sh [out-dir]   (default: listening/02-irodori-steps)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="${1:-listening/02-irodori-steps}"
REF_WAV="${IRODORI_REF_WAV:-vendor/irodori-ref.wav}"
STEPS_LOW="${AB_STEPS_LOW:-5}"
STEPS_HIGH="${AB_STEPS_HIGH:-40}"
LUFS="${AB_LUFS:--20}"
CARGO_HOME="${CARGO_HOME:-$ROOT/.cargo-home}"
export CARGO_HOME

if [[ ! -f "$REF_WAV" ]]; then
    echo "[error] reference WAV not found: $REF_WAV" >&2
    exit 3
fi

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

synth() {
    local steps="$1" out="$2" log="$3" text="$4"
    if ! cargo run --release --features onnx,wav --quiet --example irodori_demo -- \
        --text "$text" --ref-wav "$REF_WAV" --steps "$steps" --seed 0 --repeats 1 \
        --out "$out" >"$log" 2>&1; then
        echo "[error] synthesis failed (steps=$steps)" >&2
        tail -5 "$log" >&2
        exit 1
    fi
}

measure() {
    cargo run --release --features onnx,wav --quiet --example prep_ab_audio -- \
        --input "$1" --measure 2>/dev/null
}

prep() {
    local input="$1" out="$2" log="$3"
    if ! cargo run --release --features onnx,wav --quiet --example prep_ab_audio -- \
        --input "$input" --out "$out" --rate 48000 --lufs="$LUFS" >"$log" 2>&1; then
        echo "[error] prep failed" >&2
        tail -5 "$log" >&2
        exit 1
    fi
}

for i in "${!texts[@]}"; do
    text="${texts[$i]}"
    n="$(printf '%02d' $((i + 1)))"
    dir="$OUT/pair-$n"
    mkdir -p "$dir"

    echo "[pair-$n] steps=$STEPS_LOW ..."
    synth "$STEPS_LOW" "$dir/.$STEPS_LOW.raw.wav" "$dir/.$STEPS_LOW.log" "$text"
    echo "[pair-$n] steps=$STEPS_HIGH ..."
    synth "$STEPS_HIGH" "$dir/.$STEPS_HIGH.raw.wav" "$dir/.$STEPS_HIGH.log" "$text"
    echo "[pair-$n] prep (48 kHz, $LUFS LUFS) ..."
    prep "$dir/.$STEPS_LOW.raw.wav" "$dir/.$STEPS_LOW.wav" "$dir/.$STEPS_LOW.prep.log"
    prep "$dir/.$STEPS_HIGH.raw.wav" "$dir/.$STEPS_HIGH.wav" "$dir/.$STEPS_HIGH.prep.log"

    low_lufs=$(measure "$dir/.$STEPS_LOW.wav" | awk '{print $1}' | cut -d= -f2)
    high_lufs=$(measure "$dir/.$STEPS_HIGH.wav" | awk '{print $1}' | cut -d= -f2)
    echo "[pair-$n] achieved LUFS: $STEPS_LOW=$low_lufs  $STEPS_HIGH=$high_lufs"
    if ! python3 -c 'import sys; a, b = float(sys.argv[1]), float(sys.argv[2]); sys.exit(0 if abs(a - b) <= 0.3 else 1)' "$low_lufs" "$high_lufs"; then
        echo "[error] pair-$n is not loudness-matched (${low_lufs} vs ${high_lufs} LUFS)" >&2
        exit 1
    fi

    # Randomize which step count is A and which is B, per pair.
    if (( RANDOM % 2 )); then
        a="$STEPS_LOW"
        b="$STEPS_HIGH"
    else
        a="$STEPS_HIGH"
        b="$STEPS_LOW"
    fi
    mv "$dir/.$a.wav" "$dir/A.wav"
    mv "$dir/.$b.wav" "$dir/B.wav"
    rm -f "$dir"/.*.log "$dir"/.*.raw.wav

    printf '{"pair": "pair-%s", "text": %s, "A": "%s", "B": "%s"}\n' \
        "$n" \
        "$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1], ensure_ascii=False))' "$text")" \
        "steps-$a" "steps-$b" >> "$OUT/key.ndjson"
done

echo
echo "done: $OUT/"
echo "  pair-NN/A.wav  pair-NN/B.wav   step counts randomized per pair"
echo "  key.ndjson                     mapping (gitignored; open only after scoring)"
ls "$OUT"