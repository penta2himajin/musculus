//! The `musculus` CLI entry point. Built only with the `cli` feature.

use clap::{Parser, Subcommand};

/// A programmable text-to-speech framework (M0 scaffold).
#[derive(Parser)]
#[command(name = "musculus", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Synthesize text into a WAV file (the synthesis adapters land in M1).
    Synth {
        /// Text to synthesize; omit and pass --file instead.
        text: Option<String>,
        /// Read the text from a UTF-8 file instead of the argument.
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        /// Output WAV path.
        #[arg(long, default_value = "out.wav")]
        out: std::path::PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Synth { .. } => {
            eprintln!("musculus synth: synthesis adapters land in M1 (SBV2JE via ONNX).");
            eprintln!("Status and plan: docs/spec.md §6.");
            std::process::exit(2);
        }
    }
}
