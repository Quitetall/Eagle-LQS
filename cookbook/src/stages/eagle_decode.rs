// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Stage: decode an encoded blob and verify lossless roundtrip.

use std::process::Command;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};

use crate::artifacts::eagle::hash_signal;
use crate::artifacts::{EncodedBlob, RoundtripResult};
use crate::backends::eagle::runner::{read_edf_channels, resolve_lml_bin, ScratchDir};
use crate::errors;

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    /// Codec name (for metadata).
    #[serde(default = "default_codec")]
    pub codec_name: String,
    /// Original signal channels (for roundtrip comparison).
    /// Populated by the recipe compile step.
    #[serde(default)]
    pub original_channels: Vec<Vec<i64>>,
}

fn default_codec() -> String { "lamquant-lossless".into() }

pub struct EagleDecode;

#[async_trait]
impl Stage for EagleDecode {
    const NAME: &'static str = "eagle_decode";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Cpu];

    type Input = EncodedBlob;
    type Output = RoundtripResult;
    type Args = Args;

    async fn run(
        &self,
        _ctx: &StageContext,
        input: EncodedBlob,
        args: &Args,
    ) -> Result<RoundtripResult, StageError> {
        let lml_bin = resolve_lml_bin().ok_or_else(|| {
            StageError::Backend(anyhow::anyhow!(
                "lml binary not found (set LML_BIN or build LamQuant-Lossless)"
            ))
        })?;

        // Write .lml to temp dir.
        let dir = ScratchDir::new("dec").map_err(|e| StageError::Io {
            path: std::env::temp_dir(),
            source: e,
        })?;

        let lml_path = dir.join("in.lml");
        std::fs::write(&lml_path, &input.bytes).map_err(|e| StageError::Io {
            path: lml_path.clone(),
            source: e,
        })?;

        // Run lml decode --to-edf.
        let edf_path = dir.join("recon.edf");
        let output = Command::new(&lml_bin)
            .arg("decode")
            .arg(&lml_path)
            .arg("--to-edf")
            .arg("-o")
            .arg(&edf_path)
            .arg("-q")
            .output()
            .map_err(|e| StageError::Io {
                path: lml_bin.clone(),
                source: e,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StageError::Backend(anyhow::anyhow!(
                "lml decode failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }

        // Read back the reconstructed EDF.
        let decoded = read_edf_channels(&edf_path).map_err(|e| StageError::Io {
            path: edf_path,
            source: e,
        })?;

        // Compare to original.
        let n_channels = args.original_channels.len();
        let n_samples: usize = args.original_channels.iter().map(|c| c.len()).sum();
        let passed = decoded == args.original_channels;

        if !passed {
            // Find first mismatch for diagnostic.
            let detail = find_first_diff(&args.original_channels, &decoded);
            return Err(StageError::Backend(
                errors::roundtrip_failure(Self::NAME)
                    .context("ch", &n_channels.to_string())
                    .context("len", &n_samples.to_string())
                    .into_error(&detail),
            ));
        }

        Ok(RoundtripResult {
            passed,
            n_channels,
            n_samples,
            content_hash: hash_signal(&decoded),
        })
    }
}

/// Find the first difference between two signals for diagnostic output.
fn find_first_diff(original: &[Vec<i64>], decoded: &[Vec<i64>]) -> String {
    if original.len() != decoded.len() {
        return format!(
            "channel count: original={}, decoded={}",
            original.len(),
            decoded.len()
        );
    }
    for (ch, (orig, dec)) in original.iter().zip(decoded.iter()).enumerate() {
        if orig.len() != dec.len() {
            return format!(
                "ch{} length: original={}, decoded={}",
                ch,
                orig.len(),
                dec.len()
            );
        }
        for (i, (&o, &d)) in orig.iter().zip(dec.iter()).enumerate() {
            if o != d {
                return format!(
                    "ch{} sample {}: original={}, decoded={}",
                    ch, i, o, d
                );
            }
        }
    }
    "signals differ but no specific diff found".into()
}
