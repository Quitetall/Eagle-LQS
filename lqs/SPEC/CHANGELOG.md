# LQS specification changelog

All notable changes to the LQS standard are recorded here. The spec is versioned
independently of the `lqs` crate. See [`LQS-v1.0.md`](LQS-v1.0.md) §11 for the
version/stability policy.

## 1.1 — 2026-06-19 (tooling / DX — no wire-format change)

Additive developer-experience release. **The normative spec, tier thresholds,
metric formulas, codec contract, and the `LqsSubmission` JSON wire format are
unchanged from 1.0** — a 1.0 grader reads a 1.1 submission unchanged and vice
versa, so `SPEC_VERSION` stays `"1.0"`. Everything here is read-out / tooling
layered on top:

### Added
- **`eagle-lqs bench`** — grade the codec under test *and* built-in baselines
  over one corpus, ranked, with a per-codec **95% bootstrap CI** on mean R and a
  **paired sign-test** p-value versus the strongest baseline.
- **Parallel, bounded-memory corpus grading** (`rayon`) with a live progress bar
  — scales past RAM (only ~num-threads files resident); **median throughput** so
  the MiB/s figure is citable.
- **Colored terminal read-out**: unicode-boxed report, grade badges, per-band
  sparkline; **ASCII charts** (`--charts`: braille R–D scatter, per-band bars).
- **Self-contained HTML report** (`--report`): inline SVG charts (codec-CR bars,
  R–D scatter) + comparison table + per-file detail, no JS / no external assets.
- **Real `--help`** for every subcommand (clap); the legacy positional form is
  preserved.
- **`LQS-Bench-v1`** — the canonical, hash-pinned, publicly-downloadable
  benchmark corpus (PhysioNet CHB-MIT subset) under `bench/LQS-Bench-v1/`, with a
  fetch + lock recipe; the in-repo synthetic corpus is the offline
  **LQS-Bench-mini** default.

### Notes
- Confidence intervals + significance are computed at render time from the
  per-file reports; they are not stored in the submission (hence no schema bump).
  `throughput_mibs` is now rounded to 0.001 MiB/s (stable + JSON-round-trip safe).

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
