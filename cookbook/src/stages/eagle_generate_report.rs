// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Stage: aggregate roundtrip results into an AuditReport.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};

use crate::artifacts::eagle::hash_bytes;
use crate::artifacts::{AuditReport, RoundtripEntry, RoundtripResult};

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    /// Output path for the report JSON. None = write to ctx.stage_dir.
    #[serde(default)]
    pub output_path: Option<String>,
}

pub struct EagleGenerateReport;

#[async_trait]
impl Stage for EagleGenerateReport {
    const NAME: &'static str = "eagle_generate_report";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Cpu];

    type Input = RoundtripResult;
    type Output = AuditReport;
    type Args = Args;

    async fn run(
        &self,
        ctx: &StageContext,
        input: RoundtripResult,
        args: &Args,
    ) -> Result<AuditReport, StageError> {
        let entry = RoundtripEntry {
            n_channels: input.n_channels,
            n_samples: input.n_samples,
            passed: input.passed,
        };

        let total = 1;
        let passed = if input.passed { 1 } else { 0 };
        let failed = total - passed;

        let path = match &args.output_path {
            Some(p) => PathBuf::from(p),
            None => ctx.stage_dir.join("audit_report.json"),
        };

        let content_hash = hash_bytes(format!("{:?}", entry).as_bytes());

        let report = AuditReport {
            total,
            passed,
            failed,
            entries: vec![entry],
            path,
            content_hash,
        };

        // Write report JSON to disk.
        let json = serde_json::to_string_pretty(&report).map_err(|e| {
            StageError::Backend(anyhow::anyhow!("failed to serialize report: {e}"))
        })?;
        std::fs::write(&report.path, &json).map_err(|e| StageError::Io {
            path: report.path.clone(),
            source: e,
        })?;

        Ok(report)
    }
}
