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

/// Read a WAV file (float32 or int16 PCM) as mono.
///
/// Stereo input takes the first channel; the reference-voice path in
/// `src/irodori` documents that expectation.
pub fn read_wav(path: impl AsRef<std::path::Path>) -> std::io::Result<AudioChunk> {
    let reader = hound::WavReader::open(path).map_err(hound_err)?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<f32>, _>>()
            .map_err(hound_err)?,
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let raw: Vec<i16> = reader
                .into_samples::<i16>()
                .collect::<Result<Vec<i16>, _>>()
                .map_err(hound_err)?;
            let scale = if bits == 8 { 128.0 } else { 32768.0 };
            raw.into_iter().map(|v| v as f32 / scale).collect()
        }
    };
    let samples = if channels > 1 {
        samples
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % channels == 0)
            .map(|(_, v)| v)
            .collect()
    } else {
        samples
    };
    Ok(AudioChunk {
        samples,
        sample_rate,
    })
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
