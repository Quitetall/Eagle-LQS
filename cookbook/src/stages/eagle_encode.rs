// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Stage: encode a test signal through the `lml` binary.

use std::process::Command;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};

use crate::artifacts::eagle::hash_bytes;
use crate::artifacts::{EncodedBlob, TestSignal};
use crate::backends::eagle::runner::{resolve_lml_bin, write_edf_bytes, ScratchDir};

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    /// Codec name (for metadata; the binary is always `lml`).
    #[serde(default = "default_codec")]
    pub codec_name: String,
}

fn default_codec() -> String { "lamquant-lossless".into() }

pub struct EagleEncode;

#[async_trait]
impl Stage for EagleEncode {
    const NAME: &'static str = "eagle_encode";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Cpu];

    type Input = TestSignal;
    type Output = EncodedBlob;
    type Args = Args;

    async fn run(
        &self,
        _ctx: &StageContext,
        input: TestSignal,
        args: &Args,
    ) -> Result<EncodedBlob, StageError> {
        let lml_bin = resolve_lml_bin().ok_or_else(|| {
            StageError::Backend(anyhow::anyhow!(
                "lml binary not found (set LML_BIN or build LamQuant-Lossless)"
            ))
        })?;

        // Write EDF to temp dir.
        let dir = ScratchDir::new("enc").map_err(|e| StageError::Io {
            path: std::env::temp_dir(),
            source: e,
        })?;

        let edf = write_edf_bytes(&input.channels, input.fs).ok_or_else(|| {
            StageError::BadInput("signal cannot be expressed as EDF (non-i16 or empty)".into())
        })?;

        let edf_path = dir.join("in.edf");
        std::fs::write(&edf_path, &edf).map_err(|e| StageError::Io {
            path: edf_path.clone(),
            source: e,
        })?;

        // Run lml encode.
        let output = Command::new(&lml_bin)
            .arg("encode")
            .arg(&edf_path)
            .arg("-o")
            .arg(dir.join("out"))
            .arg("--no-bundle")
            .arg("--i-understand-data-loss")
            .arg("-q")
            .output()
            .map_err(|e| StageError::Io {
                path: lml_bin.clone(),
                source: e,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StageError::Backend(anyhow::anyhow!(
                "lml encode failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }

        // Read the .lml output.
        let lml_path = dir.join("in.lml");
        let bytes = std::fs::read(&lml_path).map_err(|e| StageError::Io {
            path: lml_path,
            source: e,
        })?;

        if bytes.is_empty() {
            return Err(StageError::Backend(anyhow::anyhow!(
                "lml encode produced empty output"
            )));
        }

        let content_hash = hash_bytes(&bytes);

        Ok(EncodedBlob {
            bytes,
            codec_name: args.codec_name.clone(),
            content_hash,
        })
    }
}
