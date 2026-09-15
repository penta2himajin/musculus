#!/usr/bin/env bash
#
# Blind A/B: one engine (SBV2), whole-text vs sentence-split synthesis —
# the "breathless delivery" hypothesis (docs/benchmarks/listening-log.md
# entry 5: SBV2 showed a strained delivery in 2 of 5 samples; the
# reference implementations split long text and join with silence).
#
# Same engine, same text; one side is synthesized in a single pass and
# the other is split into sentences joined with 0.4 s of silence. Both
# sides are put in one comparable container: 48 kHz mono, normalized to a
# common target and peak-limited. The split side is inherently longer by
# the inserted silences -- that is part of the intervention being judged.
#
# Loudness is matched and verified pair by pair before anything is
# presented: an unmatched A/B hands the louder side the win for the wrong
# reason (measured once: a -16 LUFS target left a peaky side 3.6 dB
# short).
#
# Writes listening/02-irodori-steps/pair-NN/{A,B}.wav and key.ndjson (gitignored,
# revealed only after scoring).
#
# Usage: scripts/make_ab_test_steps.sh [out-dir]   (default: listening/02-irodori-steps)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="${1:-listening/03-sbv2-whole-vs-split}"
REF_WAV="${IRODORI_REF_WAV:-vendor/irodori-ref.wav}"
SENTENCE_SILENCE="${AB_SENTENCE_SILENCE:-0.4}"
LUFS="${AB_LUFS:--16}"
CARGO_HOME="${CARGO_HOME:-$ROOT/.cargo-home}"
export CARGO_HOME

if [[ ! -f "$REF_WAV" ]]; then
    echo "[error] reference WAV not found: $REF_WAV" >&2
    exit 3
fi

texts=(
    "その森には、古い言い伝えがありました。月が最も高く昇る夜、静かに耳を澄ませば、風の歌声が聞こえるというのです。"
    "現在の時刻およびドル円の為替をお知らせします。今の時刻は、午後3時です。現在の為替は、1ドル152円です。"
    "こんにちは。今日はとても良い天気ですね。"
    "会議の資料を確認しました。修正が必要な箇所が三つあります。明日までに対応します。"
    "昨日は雨でしたが、今日は晴れました。週末は外出する予定です。"
)

rm -rf "$OUT"
mkdir -p "$OUT"
: > "$OUT/key.ndjson"

synth() {
    local mode="$1" out="$2" log="$3" text="$4"
    # Portable on bash 3.2 (macOS): an empty array under `set -u` is an
    # unbound-variable error, so build the extra flags as words instead.
    local extra=""
    if [[ "$mode" == "split" ]]; then
        extra="--split-sentences --sentence-silence $SENTENCE_SILENCE"
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
    echo "[pair-$n] split ..."
    synth "split" "$dir/.split.raw.wav" "$dir/.split.log" "$text"
    echo "[pair-$n] prep (48 kHz, $LUFS LUFS) ..."
    prep "$dir/.whole.raw.wav" "$dir/.whole.wav" "$dir/.whole.prep.log"
    prep "$dir/.split.raw.wav" "$dir/.split.wav" "$dir/.split.prep.log"

    whole_lufs=$(measure "$dir/.whole.wav" | awk '{print $1}' | cut -d= -f2)
    split_lufs=$(measure "$dir/.split.wav" | awk '{print $1}' | cut -d= -f2)
    echo "[pair-$n] achieved LUFS: whole=$whole_lufs  split=$split_lufs"

    # Peak limiting can leave a peaky side short of the target (measured:
    # -16 LUFS left a pair 0.8 dB apart). Lower the target below the
    # quieter side and re-prep both: with no limiting they land together.
    if ! python3 -c 'import sys; a, b = float(sys.argv[1]), float(sys.argv[2]); sys.exit(0 if abs(a - b) <= 0.3 else 1)' "$whole_lufs" "$split_lufs"; then
        lower=$(python3 -c 'import sys; print(f"{min(float(sys.argv[1]), float(sys.argv[2])) - 0.2:.2f}")' "$whole_lufs" "$split_lufs")
        echo "[pair-$n] re-matching at $lower LUFS (limiting was in play)"
        prep "$dir/.whole.raw.wav" "$dir/.whole.wav" "$dir/.whole.prep.log" "$lower"
        prep "$dir/.split.raw.wav" "$dir/.split.wav" "$dir/.split.prep.log" "$lower"
        whole_lufs=$(measure "$dir/.whole.wav" | awk '{print $1}' | cut -d= -f2)
        split_lufs=$(measure "$dir/.split.wav" | awk '{print $1}' | cut -d= -f2)
        echo "[pair-$n] achieved LUFS after re-match: whole=$whole_lufs  split=$split_lufs"
        if ! python3 -c 'import sys; a, b = float(sys.argv[1]), float(sys.argv[2]); sys.exit(0 if abs(a - b) <= 0.3 else 1)' "$whole_lufs" "$split_lufs"; then
            echo "[error] pair-$n is not loudness-matched (${whole_lufs} vs ${split_lufs} LUFS)" >&2
            exit 1
        fi
    fi

    # Randomize which step count is A and which is B, per pair.
    if (( RANDOM % 2 )); then
        a="whole"
        b="split"
    else
        a="split"
        b="whole"
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
echo "  pair-NN/A.wav  pair-NN/B.wav   whole/split randomized per pair"
echo "  key.ndjson                     mapping (gitignored; open only after scoring)"
ls "$OUT"