# LQS specification changelog

All notable changes to the LQS standard are recorded here. The spec is versioned
independently of the `lqs` crate. See [`LQS-v1.0.md`](LQS-v1.0.md) §11 for the
version/stability policy.

## 1.0 — 2026-06-18

First frozen, versioned release of the standard.

### Added
- **Versioned normative spec** (`LQS-v1.0.md`): the five tiers (L/C/M/A/below),
  metric definitions, per-band fidelity, verify-don't-trust, and grade dispatch —
  promoted from the living `STANDARD.md` and frozen.
- **Codec-conformance contract** (§6): a file-based CLI contract
  (`encode <in> <out>` / `decode <in> <out> --channels --samples --rate --dtype`)
  that makes ANY codec, in ANY language, gradable without source integration.
- **Codec manifest** (§7) and **corpus manifest** (§8) schemas, with JSON Schema
  mirrors under `schemas/`.
- **Results-submission envelope** (§9): `LqsSubmission` wrapping per-file
  `LqsReport`s (each now carrying `spec_version`), the corpus summary, and an
  optional `task_concordance` block.
- **Optional task-concordance axis** (§10): codec-agnostic downstream-task
  preservation, reported separately and explicitly **out of the tier gates**.
- **Version/stability policy** (§11): SemVer-style; any threshold/metric/L-gate
  change is a major (v2.0) bump.

### Notes
- Canonical thresholds remain `src/levels.rs`; the Python mirror
  (`lamquant_codec/lqs.py`) stays in parity and stamps `LQS_SPEC_VERSION = "1.0"`.
- The one documented Rust↔Python divergence (L-tier `min_cr`: Rust 0.8 exact-zero
  vs the Python ancestor) is unchanged and intentional.
