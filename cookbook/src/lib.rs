// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Eagle — LamQuant's testing/validation/benchmarking BLUT cookbook.
//!
//! Eagle is a BLUT cookbook (like blut-lamquant, blut-lamu) that tests
//! LamQuant's codecs through the LQS/OpenECS grading infrastructure.
//!
//! Recipes:
//!   - `eagle-codec` — lossless codec roundtrip verification
//!
//! Usage:
//!   `blut recipe run eagle-codec --args '{"channels":4,"samples":4096}'`

pub mod artifacts;
pub mod backends;
pub mod errors;
pub mod recipes;
pub mod stages;

use std::sync::Arc;

use blut::framework::{Cookbook, Registry, StageDyn};
use blut::framework::stage::ErasedStageCtor;
use blut::recipes::recipe::RecipeDef;

use crate::recipes::EAGLE_RECIPES;
use crate::stages::{EagleDecode, EagleEncode, EagleFixtureSource, EagleGenerateReport};

/// Stage constructor table — maps stage NAME → ctor closure.
pub static EAGLE_STAGES_ERASED: &[(&str, ErasedStageCtor)] = &[
    (
        <EagleFixtureSource as blut::framework::Stage>::NAME,
        || Arc::new(EagleFixtureSource) as Arc<dyn StageDyn>,
    ),
    (
        <EagleEncode as blut::framework::Stage>::NAME,
        || Arc::new(EagleEncode) as Arc<dyn StageDyn>,
    ),
    (
        <EagleDecode as blut::framework::Stage>::NAME,
        || Arc::new(EagleDecode) as Arc<dyn StageDyn>,
    ),
    (
        <EagleGenerateReport as blut::framework::Stage>::NAME,
        || Arc::new(EagleGenerateReport) as Arc<dyn StageDyn>,
    ),
];

/// Eagle cookbook — registered with BLUT's Registry.
pub struct EagleCookbook;

impl Cookbook for EagleCookbook {
    fn name(&self) -> &'static str {
        "eagle"
    }

    fn recipes(&self) -> &'static [&'static RecipeDef] {
        EAGLE_RECIPES
    }

    fn default_args(&self, _recipe: &str) -> Option<String> {
        None
    }

    fn stages_erased(&self) -> &'static [(&'static str, ErasedStageCtor)] {
        EAGLE_STAGES_ERASED
    }
}

/// Create a BLUT Registry with the Eagle cookbook registered.
pub fn registry() -> Registry {
    let mut r = Registry::new();
    r.register(Box::new(EagleCookbook));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_erased_is_complete_unique_and_self_consistent() {
        // Every entry must produce a stage whose NAME matches its key.
        assert_eq!(EAGLE_STAGES_ERASED.len(), 4, "expected 4 stages");
        let mut names: Vec<&str> = Vec::new();
        for &(key, ctor) in EAGLE_STAGES_ERASED {
            let stage = ctor();
            assert_eq!(
                stage.name(),
                key,
                "stage ctor for '{key}' produced '{}'",
                stage.name()
            );
            names.push(key);
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 4, "stage names must be unique");
    }

    #[test]
    fn eagle_codec_recipe_compiles() {
        use blut::recipes::recipe::Recipe;
        let recipe = crate::recipes::EagleCodec;
        let args = crate::recipes::eagle_codec::Args {
            channels: 4,
            samples: 4096,
            sample_rate: 256.0,
        };
        let plan = recipe.compile(args);
        assert!(plan.is_ok(), "eagle-codec recipe failed to compile: {:?}", plan.err());
    }

    #[test]
    fn cookbook_metadata_is_well_formed() {
        let cb = EagleCookbook;
        assert_eq!(cb.name(), "eagle");
        assert!(!cb.recipes().is_empty(), "must have at least one recipe");
        assert!(!cb.stages_erased().is_empty(), "must have at least one stage");
    }
}
