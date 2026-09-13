//! WAV file I/O for the CLI and the evaluation harness.
//!
//! The SBV2 decode output is 32-bit float PCM at its native rate; this
//! module writes that faithfully. Reading arrives with the M2
//! evaluation harness.

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::types::AudioChunk;

fn hound_err(e: hound::Error) -> std::io::Error {
    std::io::Error::other(e)
}

/// Write one chunk of audio to a float WAV file.
pub fn write_wav(path: impl AsRef<std::path::Path>, chunk: &AudioChunk) -> std::io::Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: chunk.sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).map_err(hound_err)?;
    for &sample in &chunk.samples {
        writer.write_sample(sample).map_err(hound_err)?;
    }
    writer.finalize().map_err(hound_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_readable_float_wav() {
        let dir = std::env::temp_dir().join("musculus-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.wav");
        let chunk = AudioChunk {
            samples: vec![0.0, 0.5, -0.5, 0.25],
            sample_rate: 44_100,
        };
        write_wav(&path, &chunk).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().sample_rate, 44_100);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_format, SampleFormat::Float);
        let samples: Vec<f32> = reader.into_samples::<f32>().map(|s| s.unwrap()).collect();
        assert_eq!(samples, chunk.samples);
        let _ = std::fs::remove_file(&path);
    }
}
