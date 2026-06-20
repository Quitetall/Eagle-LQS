// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Compatible<EagleBackend> marker impls for all Eagle stages.

use blut::framework::Compatible;

use crate::backends::EagleBackend;
use crate::stages::{EagleDecode, EagleEncode, EagleFixtureSource, EagleGenerateReport};

impl Compatible<EagleBackend> for EagleFixtureSource {}
impl Compatible<EagleBackend> for EagleEncode {}
impl Compatible<EagleBackend> for EagleDecode {}
impl Compatible<EagleBackend> for EagleGenerateReport {}
