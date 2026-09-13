# musculus

A programmable text-to-speech framework — normalization, synthesis, and audio output as composable adapters.

> **musculus** is named after the house mouse *Mus musculus* — *musculus* is Latin for "muscle".
> mouth → mouse → *musculus* — a chain from speech to the framework's identity, mirroring
> euhadra's ear → cochlea → snail → *Euhadra*. [euhadra](https://github.com/penta2himajin/euhadra)
> is the listening half; musculus is the speaking half.

## What it does

```
テキスト入力
    → SpeechNormalizer   (読み展開: 数値・記号・日付・漢字読み)
    → TextProcessor      (ユーザ辞書・表記揺れ)
    → TtsAdapter         (ローカル合成エンジン: ONNX)
    → AudioEmitter       (再生 / WAV / stdout)
```

Each stage is a Rust trait. Swap any component without touching the rest. Local-first:
synthesis runs natively on ONNX Runtime; nothing ships with the library, and model weights
are fetched by setup scripts like euhadra's.

## Status

**M0 — scaffolding.** Design documents are in `docs/`:

- [docs/spec.md](docs/spec.md) — architecture, engine decisions (ja baseline: Style-Bert-VITS2 JP-Extra), milestones
- [docs/evaluation.md](docs/evaluation.md) — L1/L2/L3 evaluation policy (round-trip CER, normalization F1, proxy MOS)
- [docs/decisions/](docs/decisions/) — ADRs

## Setup

```bash
git config core.hooksPath git-hooks
```

## Build & Test

```bash
cargo build --workspace
cargo test  --workspace
```

## License

MIT. See `LICENSE`.