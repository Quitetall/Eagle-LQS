// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Eagle error codes — structured failures for test stages.
//!
//! Uses BLUT's [`blut::framework::StageFailure`] for structured error
//! context. Each error code maps to an invariant in the ADR 0056
//! taxonomy. Stages create `StageFailure` values and wrap them in
//! `StageError::Backend(...)` so BLUT lineage stores the structured
//! data natively.

use blut::framework::{Severity, StageFailure};

/// Domain name for Eagle errors.
pub const DOMAIN: &str = "eagle";

// ── Error codes ────────────────────────────────────────────────────

/// decode(encode(x)) != x — the fundamental lossless invariant.
pub const E_ROUNDTRIP: &str = "E_ROUNDTRIP";

/// Invalid input was accepted (should have been rejected).
pub const E_REJECT: &str = "E_REJECT";

/// Metric below a required floor (CR, R, PRD, etc.).
pub const E_THRESHOLD: &str = "E_THRESHOLD";

/// Cross-backend or cross-language divergence.
pub const E_PARITY: &str = "E_PARITY";

/// Wire format / schema mismatch.
pub const E_FORMAT: &str = "E_FORMAT";

// ── ErrorDomain registration ───────────────────────────────────────

/// Eagle error domain — registered with BLUT's error infrastructure.
pub struct EagleErrorDomain;

impl blut::framework::ErrorDomain for EagleErrorDomain {
    const NAME: &'static str = DOMAIN;
    const CODES: &[(&'static str, &'static str)] = &[
        (E_ROUNDTRIP, "decode(encode(x)) != x"),
        (E_REJECT, "invalid input was accepted"),
        (E_THRESHOLD, "metric below required floor"),
        (E_PARITY, "cross-backend or cross-language divergence"),
        (E_FORMAT, "wire format / schema mismatch"),
    ];
}

// ── Convenience constructors ───────────────────────────────────────

/// Create a roundtrip failure.
pub fn roundtrip_failure(stage: &str) -> StageFailure {
    StageFailure::new(E_ROUNDTRIP, DOMAIN).stage(stage).severity(Severity::Major)
}

/// Create a rejection failure.
pub fn reject_failure(stage: &str) -> StageFailure {
    StageFailure::new(E_REJECT, DOMAIN).stage(stage).severity(Severity::Major)
}

/// Create a threshold failure.
pub fn threshold_failure(stage: &str) -> StageFailure {
    StageFailure::new(E_THRESHOLD, DOMAIN).stage(stage).severity(Severity::Major)
}

/// Create a parity failure.
pub fn parity_failure(stage: &str) -> StageFailure {
    StageFailure::new(E_PARITY, DOMAIN).stage(stage).severity(Severity::Major)
}

/// Create a format failure.
pub fn format_failure(stage: &str) -> StageFailure {
    StageFailure::new(E_FORMAT, DOMAIN).stage(stage).severity(Severity::Major)
}
