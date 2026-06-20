// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Recipe: `eagle-codec` — lossless codec test suite.
//!
//! Pipeline: `FixtureSource → Encode → Decode → GenerateReport`.
//!
//! Generates a deterministic test signal, encodes it through the real
//! `lml` binary, decodes it back, and verifies bit-exact roundtrip.
//! Produces an AuditReport as the final artifact.

use serde::{Deserialize, Serialize};

use blut::framework::error::RecipeError;
use blut::framework::plan::Plan;
use blut::recipes::recipe::{Course, Recipe};

use crate::backends::EagleBackend;
use crate::stages::{
    eagle_decode, eagle_encode, eagle_fixture_source, eagle_generate_report, EagleDecode,
    EagleEncode, EagleFixtureSource, EagleGenerateReport,
};

#[derive(Default)]
pub struct EagleCodec;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    /// Number of channels.
    #[serde(default = "default_channels")]
    pub channels: usize,
    /// Samples per channel.
    #[serde(default = "default_samples")]
    pub samples: usize,
    /// Sample rate in Hz.
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
}

fn default_channels() -> usize { 4 }
fn default_samples() -> usize { 4096 }
fn default_sample_rate() -> f64 { 256.0 }

impl Recipe for EagleCodec {
    const NAME: &'static str = "eagle-codec";
    const DESCRIPTION: &'static str =
        "Lossless codec test suite — roundtrip verification through real lml binary";
    const CATEGORY: Course = Course::Eval;
    const INPUT_KINDS: &'static [&'static str] = &[];
    const OUTPUT_KIND: &'static str = "eagle.audit_report";

    type Backend = EagleBackend;
    type Args = Args;

    fn compile(&self, args: Self::Args) -> Result<Plan<(), Self::Backend>, RecipeError> {
        let fixture_args = eagle_fixture_source::Args {
            channels: args.channels,
            samples: args.samples,
            sample_rate: args.sample_rate,
        };
        let encode_args = eagle_encode::Args {
            codec_name: "lamquant-lossless".into(),
        };
        // Decode args are a placeholder — the original signal is passed
        // through the plan edges, not through args. The decode stage
        // receives the EncodedBlob as input and the original signal is
        // embedded in the stage's args by the plan builder. However, since
        // Plan::then only passes the Output of the previous stage as Input,
        // we need to carry the original signal differently.
        //
        // For now, the decode args carry the original signal channels
        // directly (populated at compile time from the fixture args).
        // This is a known limitation — a fork/merge pattern would be
        // cleaner but adds complexity for the first version.
        let decode_args = eagle_decode::Args {
            codec_name: "lamquant-lossless".into(),
            original_channels: generate_fixture(&args),
        };
        let report_args = eagle_generate_report::Args {
            output_path: None,
        };

        let plan = Plan::new(Self::NAME, serde_json::to_value(&args).unwrap())
            .start(EagleFixtureSource, fixture_args)
            .then(EagleEncode, encode_args)
            .then(EagleDecode, decode_args)
            .then(EagleGenerateReport, report_args)
            .finish();

        Ok(plan)
    }
}

blut::register_recipe!(EagleCodec);

/// Generate the same deterministic fixture as EagleFixtureSource,
/// used to populate the decode stage's args with the original signal.
fn generate_fixture(args: &Args) -> Vec<Vec<i64>> {
    let fs = args.sample_rate;
    (0..args.channels)
        .map(|c| {
            let amp = 1.0 + 0.25 * c as f64;
            (0..args.samples)
                .map(|i| {
                    let t = i as f64 / fs;
                    let v = amp
                        * (80.0 * (2.0 * std::f64::consts::PI * 2.0 * t).sin()
                            + 50.0 * (2.0 * std::f64::consts::PI * 10.0 * t).sin()
                            + 25.0 * (2.0 * std::f64::consts::PI * 22.0 * t).sin()
                            + 10.0 * (2.0 * std::f64::consts::PI * 40.0 * t).sin());
                    v.round() as i64
                })
                .collect()
        })
        .collect()
}
