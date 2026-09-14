#!/usr/bin/env python3
"""Generate the accent regression fixture from the OpenJTalk reference.

Writes tests/evaluation/annotations/ja_accent_reference.jsonl, which
tests/l3_accent.rs compares musculus's frontend against. The oracle is
pyopenjtalk; the header line records its version so a fixture refresh is
traceable (the same discipline as the CER ruler's version in the baseline
JSON, docs/evaluation.md).

Run from the repository root with a venv that has pyopenjtalk:

    python3 -m venv .venv && .venv/bin/pip install pyopenjtalk
    .venv/bin/python scripts/gen_accent_reference.py

The fixture is data the project owns (it stores reference accent values for
sentences we chose), not a copy of any dictionary.
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys

OUT = pathlib.Path("tests/evaluation/annotations/ja_accent_reference.jsonl")

# Items chosen to cover the cases the accent work touches: polite prefixes,
# numerals (bare and compounded), a plain sentence, and the phrase the
# listener flagged.
ITEMS = [
    "こんにちは",
    "注文",
    "ご注文",
    "ご注文は",
    "お名前",
    "お勉強",
    "ご指導",
    "ご連絡",
    "千円",
    "二千円",
    "千二百",
    "千五百",
    "二千二百",
    "二千二百円",
    "1,200",
    "1,200円",
    "先週と比べると",
    "猫が座って動かない。",
]


def oracle_version() -> dict:
    """pyopenjtalk (and the OpenJTalk it wraps) versions for the header."""
    try:
        import pyopenjtalk

        version = getattr(pyopenjtalk, "__version__", "unknown")
        openjtalk = getattr(pyopenjtalk, "OPEN_JTALK_DICT_DIR", "")
    except Exception as error:  # pragma: no cover - diagnostics only
        version, openjtalk = f"unknown ({error})", ""
    pip = subprocess.run(
        [sys.executable, "-m", "pip", "show", "pyopenjtalk"],
        capture_output=True,
        text=True,
    ).stdout
    return {
        "oracle": "pyopenjtalk",
        "pyopenjtalk_version": version,
        "pip_show": pip.strip().splitlines()[:4],
        "open_jtalk_dict_dir": str(openjtalk),
        "generator": "scripts/gen_accent_reference.py",
    }


def main() -> int:
    import pyopenjtalk

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", encoding="utf-8") as handle:
        handle.write(json.dumps({"_meta": oracle_version()}, ensure_ascii=False) + "\n")
        for item in ITEMS:
            nodes = pyopenjtalk.run_frontend(item)
            handle.write(
                json.dumps(
                    {
                        "input": item,
                        "nodes": [
                            {
                                "surface": node.get("string", ""),
                                "pos": node.get("pos", ""),
                                "pron": node.get("pron", ""),
                                "accent": node.get("acc", 0),
                                "mora_size": node.get("mora_size", 0),
                                "chain": node.get("chain_rule", "*"),
                            }
                            for node in nodes
                        ],
                    },
                    ensure_ascii=False,
                )
                + "\n"
            )
    print(f"wrote {OUT} ({len(ITEMS)} items)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
