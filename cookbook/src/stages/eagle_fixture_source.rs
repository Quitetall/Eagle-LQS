// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Stage: generate a deterministic multi-channel test signal.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};

use crate::artifacts::TestSignal;

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

pub struct EagleFixtureSource;

#[async_trait]
impl Stage for EagleFixtureSource {
    const NAME: &'static str = "eagle_fixture_source";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Cpu];

    type Input = ();
    type Output = TestSignal;
    type Args = Args;

    async fn run(
        &self,
        _ctx: &StageContext,
        _input: (),
        args: &Args,
    ) -> Result<TestSignal, StageError> {
        // Deterministic multi-band sinusoid mix — same as the existing
        // `fixture()` in the eagle adapter. No RNG.
        let fs = args.sample_rate;
        let channels: Vec<Vec<i64>> = (0..args.channels)
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
            .collect();

        Ok(TestSignal::new(channels, fs))
    }
}
