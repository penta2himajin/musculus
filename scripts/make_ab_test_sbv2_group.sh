#!/usr/bin/env bash
#
# Blind A/B: one engine (SBV2), whole-text vs length-aware grouping.
#
# The previous A/B (whole vs per-sentence split) came out 3-2 with a mean
# CMOS of exactly 0.00: splitting helped long multi-sentence text but hurt
# short text and introduced its own defects. This set tests the middle
# ground: group short sentences up to AB_MAX_CHARS and cut only when a
# group has grown long.
#
# Same engine, same text; one side is synthesized in a single pass and
# the other is grouped into <= AB_MAX_CHARS segments joined with silence.
# Both sides are put in one comparable container: 48 kHz mono, normalized
# to a common target and peak-limited. The grouped side is inherently
# longer by the inserted silences -- that is part of the intervention.
#
# Loudness is matched and verified pair by pair before anything is
# presented: an unmatched A/B hands the louder side the win for the wrong
# reason (measured once: a -16 LUFS target left a peaky side 3.6 dB
# short).
#
# Writes ab-test-steps/pair-NN/{A,B}.wav and key.ndjson (gitignored,
# revealed only after scoring).
#
# Usage: scripts/make_ab_test_steps.sh [out-dir]   (default: ab-test-steps)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="${1:-ab-test-sbv2-group}"
REF_WAV="${IRODORI_REF_WAV:-vendor/irodori-ref.wav}"
SENTENCE_SILENCE="${AB_SENTENCE_SILENCE:-0.4}"
MAX_CHARS="${AB_MAX_CHARS:-40}"
LUFS="${AB_LUFS:--16}"
CARGO_HOME="${CARGO_HOME:-$ROOT/.cargo-home}"
export CARGO_HOME

if [[ ! -f "$REF_WAV" ]]; then
    echo "[error] reference WAV not found: $REF_WAV" >&2
    exit 3
fi

# Four texts long enough that grouping actually cuts (> MAX_CHARS), plus
# one short control: at <= MAX_CHARS the grouped side collapses to a
# single pass, so that pair measures run-to-run noise only (synthesis is
# not deterministic -- measured: same-mode pairs differ in waveform).
texts=(
    "その森には、古い言い伝えがありました。月が最も高く昇る夜、静かに耳を澄ませば、風の歌声が聞こえるというのです。"
    "現在の時刻およびドル円の為替をお知らせします。今の時刻は、午後3時です。現在の為替は、1ドル152円です。"
    "昨日の会議では、来期の予算案について議論しました。修正が必要な箇所が三つ見つかったので、明日までに対応します。"
    "現在の為替は、1ドル152円です。先週と比べると、少し円安になっているようです。"
    "こんにちは。今日はとても良い天気ですね。"
)

rm -rf "$OUT"
mkdir -p "$OUT"
: > "$OUT/key.ndjson"

synth() {
    local mode="$1" out="$2" log="$3" text="$4"
    # Portable on bash 3.2 (macOS): an empty array under `set -u` is an
    # unbound-variable error, so build the extra flags as words instead.
    local extra=""
    if [[ "$mode" == "group" ]]; then
        extra="--split-sentences --max-chars $MAX_CHARS --sentence-silence $SENTENCE_SILENCE"
    fi
    if ! cargo run --release --features cli,onnx --quiet -- \
        synth "$text" $extra --out "$out" >"$log" 2>&1; then
        echo "[error] synthesis failed (mode=$mode)" >&2
        tail -5 "$log" >&2
        exit 1
    fi
}

measure() {
    cargo run --release --features onnx,wav --quiet --example prep_ab_audio -- \
        --input "$1" --measure 2>/dev/null
}

prep() {
    local input="$1" out="$2" log="$3" target="${4:-$LUFS}"
    if ! cargo run --release --features onnx,wav --quiet --example prep_ab_audio -- \
        --input "$input" --out "$out" --rate 48000 --lufs="$target" >"$log" 2>&1; then
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

    echo "[pair-$n] whole ..."
    synth "whole" "$dir/.whole.raw.wav" "$dir/.whole.log" "$text"
    echo "[pair-$n] group (max $MAX_CHARS chars) ..."
    synth "group" "$dir/.group.raw.wav" "$dir/.group.log" "$text"
    echo "[pair-$n] prep (48 kHz, $LUFS LUFS) ..."
    prep "$dir/.whole.raw.wav" "$dir/.whole.wav" "$dir/.whole.prep.log"
    prep "$dir/.group.raw.wav" "$dir/.group.wav" "$dir/.group.prep.log"

    whole_lufs=$(measure "$dir/.whole.wav" | awk '{print $1}' | cut -d= -f2)
    group_lufs=$(measure "$dir/.group.wav" | awk '{print $1}' | cut -d= -f2)
    echo "[pair-$n] achieved LUFS: whole=$whole_lufs  group=$group_lufs"

    # Peak limiting can leave a peaky side short of the target (measured:
    # -16 LUFS left a pair 0.8 dB apart). Lower the target below the
    # quieter side and re-prep both: with no limiting they land together.
    if ! python3 -c 'import sys; a, b = float(sys.argv[1]), float(sys.argv[2]); sys.exit(0 if abs(a - b) <= 0.3 else 1)' "$whole_lufs" "$group_lufs"; then
        lower=$(python3 -c 'import sys; print(f"{min(float(sys.argv[1]), float(sys.argv[2])) - 0.2:.2f}")' "$whole_lufs" "$group_lufs")
        echo "[pair-$n] re-matching at $lower LUFS (limiting was in play)"
        prep "$dir/.whole.raw.wav" "$dir/.whole.wav" "$dir/.whole.prep.log" "$lower"
        prep "$dir/.group.raw.wav" "$dir/.group.wav" "$dir/.group.prep.log" "$lower"
        whole_lufs=$(measure "$dir/.whole.wav" | awk '{print $1}' | cut -d= -f2)
        group_lufs=$(measure "$dir/.group.wav" | awk '{print $1}' | cut -d= -f2)
        echo "[pair-$n] achieved LUFS after re-match: whole=$whole_lufs  group=$group_lufs"
        if ! python3 -c 'import sys; a, b = float(sys.argv[1]), float(sys.argv[2]); sys.exit(0 if abs(a - b) <= 0.3 else 1)' "$whole_lufs" "$group_lufs"; then
            echo "[error] pair-$n is not loudness-matched (${whole_lufs} vs ${group_lufs} LUFS)" >&2
            exit 1
        fi
    fi

    # Classify by the segment count the CLI reported, not by duration:
    # grouping can cut and still land at nearly the same length, because
    # per-segment synthesis is shorter than one pass over the same text
    # (measured: +0.4 s of inserted silence against ~0.34 s of shortening).
    group_chunks=$(grep -o '([0-9]\+ chunk' "$dir/.group.log" | head -1 | tr -dc '0-9')
    whole_chunks=$(grep -o '([0-9]\+ chunk' "$dir/.whole.log" | head -1 | tr -dc '0-9')
    group_secs=$(measure "$dir/.group.wav" | awk '{print $3}' | cut -d= -f2)
    whole_secs=$(measure "$dir/.whole.wav" | awk '{print $3}' | cut -d= -f2)
    kind=$(python3 -c 'import sys; print("signal" if int(sys.argv[1] or 1) >= 2 else "control")' "${group_chunks:-1}")
    echo "[pair-$n] kind=$kind (group ${group_chunks:-?} chunk(s) ${group_secs}s vs whole ${whole_chunks:-?} chunk(s) ${whole_secs}s)"

    # Randomize which mode is A and which is B, per pair.
    if (( RANDOM % 2 )); then
        a="whole"
        b="group"
    else
        a="group"
        b="whole"
    fi
    mv "$dir/.$a.wav" "$dir/A.wav"
    mv "$dir/.$b.wav" "$dir/B.wav"
    rm -f "$dir"/.*.log "$dir"/.*.raw.wav

    printf '{"pair": "pair-%s", "kind": "%s", "text": %s, "A": "%s", "B": "%s"}\n' \
        "$n" "$kind" \
        "$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1], ensure_ascii=False))' "$text")" \
        "$a" "$b" >> "$OUT/key.ndjson"
done

echo
echo "done: $OUT/"
echo "  pair-NN/A.wav  pair-NN/B.wav   whole/group randomized per pair"
echo "  key.ndjson                     mapping (gitignored; open only after scoring)"
echo "  (max_chars=$MAX_CHARS, silence=${SENTENCE_SILENCE}s)"
ls "$OUT"