//! `.sbv2` bundle parsing and style vectors.
//!
//! Format (reference: sbv2_core `sbv2file.rs`, MIT): a zstd-compressed
//! tar archive holding `model.onnx` (the VITS2 decode model) and
//! `style_vectors.json` (per-style vectors; row 0 is the neutral mean).
//! This is our own reader for that container.

use std::io::{Cursor, Read};

use serde::Deserialize;
use tar::Archive;
use zstd::decode_all;

use crate::traits::TtsError;

/// One voice bundle: the decode model and its style table.
#[derive(Debug)]
pub struct Sbv2Bundle {
    pub model_onnx: Vec<u8>,
    pub style_vectors: StyleVectors,
}

/// Parse a `.sbv2` binary into its model bytes and style table.
pub fn parse_sbv2file(bytes: &[u8]) -> Result<Sbv2Bundle, TtsError> {
    let decompressed = decode_all(Cursor::new(bytes))
        .map_err(|e| TtsError::ModelLoad(format!("sbv2 zstd decompress: {e}")))?;
    let mut archive = Archive::new(Cursor::new(decompressed));

    let mut model_onnx = None;
    let mut style_vectors = None;
    let entries = archive
        .entries()
        .map_err(|e| TtsError::ModelLoad(format!("sbv2 tar entries: {e}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| TtsError::ModelLoad(format!("sbv2 tar entry: {e}")))?;
        let path = String::from_utf8_lossy(entry.path_bytes().as_ref()).to_string();
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| TtsError::ModelLoad(format!("sbv2 tar read {path}: {e}")))?;
        match path.as_str() {
            "model.onnx" => model_onnx = Some(buf),
            "style_vectors.json" => style_vectors = Some(buf),
            _ => continue,
        }
    }

    let style_vectors = style_vectors
        .ok_or_else(|| TtsError::ModelLoad("sbv2 bundle lacks style_vectors.json".into()))?;
    let model_onnx =
        model_onnx.ok_or_else(|| TtsError::ModelLoad("sbv2 bundle lacks model.onnx".into()))?;
    Ok(Sbv2Bundle {
        model_onnx,
        style_vectors: StyleVectors::from_json(&style_vectors)?,
    })
}

/// Per-style vectors: row 0 is the neutral mean; other rows are styles.
#[derive(Debug, Clone)]
pub struct StyleVectors {
    shape: [usize; 2],
    data: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
struct StyleVectorsJson {
    shape: [usize; 2],
    data: Vec<Vec<f32>>,
}

impl StyleVectors {
    pub fn from_json(bytes: &[u8]) -> Result<Self, TtsError> {
        let json: StyleVectorsJson = serde_json::from_slice(bytes)
            .map_err(|e| TtsError::ModelLoad(format!("style_vectors.json: {e}")))?;
        Ok(Self {
            shape: json.shape,
            data: json.data,
        })
    }

    /// Number of styles (rows) in the table.
    pub fn style_count(&self) -> usize {
        self.shape[0]
    }

    /// The style vector for `style_id`, blended with the neutral mean
    /// by `weight`: `mean + (style − mean) × weight`.
    ///
    /// `weight = 0` gives the neutral mean, `weight = 1` the raw style.
    pub fn vector(&self, style_id: i32, weight: f32) -> Result<Vec<f32>, TtsError> {
        if style_id < 0 || style_id as usize >= self.style_count() {
            return Err(TtsError::Config(format!(
                "style id {style_id} out of range (0..{})",
                self.style_count()
            )));
        }
        let mean = &self.data[0];
        let style = &self.data[style_id as usize];
        if mean.len() != style.len() {
            return Err(TtsError::ModelLoad(
                "style vector rows have inconsistent widths".into(),
            ));
        }
        Ok(mean
            .iter()
            .zip(style.iter())
            .map(|(&m, &s)| m + (s - m) * weight)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_bundle() -> Vec<u8> {
        // Build a minimal .sbv2 in memory: zstd(tar{model.onnx, style_vectors.json})
        fn append(builder: &mut tar::Builder<Vec<u8>>, name: &str, data: &[u8]) {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, data).unwrap();
        }
        let style_json = br#"{"shape": [2, 3], "data": [[1.0, 1.0, 1.0], [2.0, 1.0, 0.0]]}"#;
        let mut builder = tar::Builder::new(Vec::new());
        append(&mut builder, "model.onnx", b"ONNXBYTES");
        append(&mut builder, "style_vectors.json", style_json);
        let tar_bytes = builder.into_inner().unwrap();
        zstd::encode_all(Cursor::new(tar_bytes), 3).unwrap()
    }

    #[test]
    fn parses_model_and_style_vectors() {
        let bundle = parse_sbv2file(&tiny_bundle()).unwrap();
        assert_eq!(bundle.model_onnx, b"ONNXBYTES");
        assert_eq!(bundle.style_vectors.style_count(), 2);
    }

    #[test]
    fn style_vector_blends_with_neutral_mean() {
        let bundle = parse_sbv2file(&tiny_bundle()).unwrap();
        // Row 0 is the mean [1, 1, 1]; row 1 is [2, 1, 0].
        assert_eq!(
            bundle.style_vectors.vector(0, 1.0).unwrap(),
            vec![1.0, 1.0, 1.0]
        );
        assert_eq!(
            bundle.style_vectors.vector(1, 1.0).unwrap(),
            vec![2.0, 1.0, 0.0]
        );
        // Half weight pulls the style halfway to the mean.
        assert_eq!(
            bundle.style_vectors.vector(1, 0.5).unwrap(),
            vec![1.5, 1.0, 0.5]
        );
        // Zero weight is the neutral mean regardless of id.
        assert_eq!(
            bundle.style_vectors.vector(1, 0.0).unwrap(),
            vec![1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn out_of_range_style_id_is_a_config_error() {
        let bundle = parse_sbv2file(&tiny_bundle()).unwrap();
        let err = bundle.style_vectors.vector(5, 1.0).unwrap_err();
        assert!(matches!(err, TtsError::Config(_)));
    }

    #[test]
    fn broken_bundle_is_a_model_load_error() {
        let err = parse_sbv2file(b"not a zstd stream").unwrap_err();
        assert!(matches!(err, TtsError::ModelLoad(_)));
    }
}
