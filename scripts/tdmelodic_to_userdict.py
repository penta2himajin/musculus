#!/usr/bin/env python3
"""Generate a jpreprocess-format user dictionary CSV from UniDic + tdmelodic.

The frontend (jpreprocess/OpenJTalk) takes accents from its dictionary, so a
standard-accent dictionary is the way to widen coverage. This tool turns a
word list into the 16-column CSV that `dict_tools build --user jpreprocess`
accepts:

    tdmelodic_to_userdict.py words.txt --out tdmelodic-userdict.csv
    dict_tools build --user jpreprocess tdmelodic-userdict.csv user.bin

Where the accent comes from:
- **UniDic kana-accent** supplies the accent and the accent-connection rule
  for words it knows (its `acc` and `concat` columns), which is most of the
  vocabulary and is licence-safe (GPLv2/LGPLv2.1/BSD);
- for tokens UniDic has no accent for, **tdmelodic's model** estimates one
  (BSD-3) and this tool derives the accent position from its marked reading.

Run it with the tdmelodic environment's interpreter (scripts/setup_tdmelodic.sh):

    PATH="$PWD/.mecab/bin:$PATH" SETUPTOOLS_USE_DISTUTILS=stdlib \\
        .venv-tdmelodic39/bin/python scripts/tdmelodic_to_userdict.py words.txt

Notes and limits, all measured (docs/accent-resources.md):
- An entry moves the realisation when the word **heads its accent phrase**;
  a multi-word compound may still need a whole-phrase entry.
- A word whose reading needs more than one accent nucleus is skipped: one
  dictionary entry is one accent phrase. Those belong to the override table.
- The connection IDs default to the general-noun band of the system
  dictionary (名詞,一般 = 1345); override them with --left/--right/--cost.
"""

from __future__ import annotations

import argparse
import csv
import pathlib
import sys

SMALL_KANA = set("ャュョァィゥェォヮ")

# NAIST-jdic connection bands, taken from the system dictionary CSV.
POS_BANDS = {
    "サ変": ("名詞", "サ変接続", "*", "*", "1343"),
    "固有名詞": ("名詞", "固有名詞", "一般", "*", "1354"),
    "一般": ("名詞", "一般", "*", "*", "1345"),
}


def count_morae(reading: str) -> int:
    """Morae in a katakana reading (small kana join the previous mora)."""
    return sum(0 if ch in SMALL_KANA and i else 1 for i, ch in enumerate(reading))


def parse_marked(marked: str) -> tuple[str, list[str]]:
    """Split tdmelodic's marked reading into a clean reading and the markers.

    tdmelodic appends a marker after the mora it belongs to: `[` opens a high
    span (the rise happens after that mora), `]` closes it (the fall happens
    after that mora). So キ[カイガ]クシュー is キ low, カイガ high.
    """
    reading = []
    markers = []
    for ch in marked:
        if ch in "[]":
            if markers:
                markers[-1] = ch
            continue
        if reading and ch in SMALL_KANA:
            reading[-1] += ch
        else:
            reading.append(ch)
            markers.append("")
    return "".join(reading), markers


def accent_from_markers(markers: list[str]) -> int | None:
    """Accent position (1-based) from the marker list, or None if unclear.

    Exactly one `]` means one nucleus: its mora is where the pitch falls.
    No `]` means the fall is after the word; that is heiban (0), which is how
    the dictionary encodes a plateau.
    """
    falls = [i for i, m in enumerate(markers) if m == "]"]
    if len(falls) == 1:
        return falls[0] + 1
    if not falls:
        return 0
    return None


def pos_fields(unidic_pos: str) -> tuple[str, str, str, str, str]:
    """UniDic's short POS → (pos1..4, connection id)."""
    key = "一般"
    if "サ変" in unidic_pos:
        key = "サ変"
    elif "固有名詞" in unidic_pos:
        key = "固有名詞"
    p1, p2, p3, p4, ident = POS_BANDS[key]
    return p1, p2, p3, p4, ident


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("words", type=pathlib.Path, help="one word per line")
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--chain-default", default="C2")
    parser.add_argument("--cost", type=int, default=3000)
    parser.add_argument("--left", default=None)
    parser.add_argument("--right", default=None)
    parser.add_argument("--report", action="store_true", help="print each entry")
    args = parser.parse_args()

    from tdmelodic.nn.convert import Converter

    converter = Converter()

    rows: dict[tuple[str, str], list[str]] = {}
    skipped: list[str] = []
    for raw in args.words.read_text(encoding="utf-8").splitlines():
        word = raw.strip()
        if not word:
            continue
        # Read the word as a whole: UniDic's best path splits compounds and
        # can mis-read the pieces (大規模言語モデル came out as 大=オー +
        # 規模), while s2ya returns the whole reading and its accent.
        marked = converter.s2ya(word)
        reading, markers = parse_marked(marked)
        accent = accent_from_markers(markers)
        if accent is None:
            skipped.append(f"{word} (multi-nucleus: {marked})")
            continue
        morae = count_morae(reading)
        if morae == 0:
            continue
        chain = "C2" if args.chain_default is None else args.chain_default
        rows[(word, reading)] = [
            word,
            args.left or "1345",
            args.right or "1345",
            str(args.cost),
            "名詞",
            "一般",
            "*",
            "*",
            "*",
            "*",
            word,
            reading,
            reading,
            f"{accent}/{morae}",
            chain,
            "-1",
        ]

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        for row in rows.values():
            writer.writerow(row)
            if args.report:
                print("  " + ",".join(row))

    print(f"{args.out}: {len(rows)} entries")
    if skipped:
        print(f"skipped {len(skipped)}:", file=sys.stderr)
        for item in skipped:
            print(f"  {item}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
