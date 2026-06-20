// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam

pub mod eagle_codec;

pub use eagle_codec::EagleCodec;

use blut::recipes::recipe::RecipeDef;

/// All Eagle recipes, registered with BLUT.
pub static EAGLE_RECIPES: &[&RecipeDef] = &[&eagle_codec::DEF];
