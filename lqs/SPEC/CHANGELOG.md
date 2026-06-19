# LQS specification changelog

All notable changes to the LQS standard are recorded here. The spec is versioned
independently of the `lqs` crate. See [`LQS-v1.0.md`](LQS-v1.0.md) §11 for the
version/stability policy.

## 1.1 — 2026-06-19 (Near-Lossless tier + developer-experience layer)

Additive minor within the **v1 major**: the manifest/submission **wire format
and structure are unchanged**, and v1.0 manifests still load. `SPEC_VERSION`
is now `"1.1"`. The one substantive standard change is a new grade value.

### Added — the **N (Near-Lossless)** tier
- A fifth tier between **L** and **C**: **R ≥ 0.99, PRD ≤ 5 %, CR ≥ 1.0**
  (no expansion), no per-band requirements — the strongest *non-lossless*
  tier, for codecs whose reconstruction is small-error but not bit-exact
  (e.g. a bounded-error near-lossless mode). Strictness order is now
  **L < N < C < M < A**. The `grade` field can now be `N`; consumers that
  switch on grades should handle it. Canonical thresholds: `src/levels.rs`;
  Python mirror tracks them.
- The "climb a tier" `violations` now report the tier **immediately above**
  the one a codec passed (a precise one-tier-up to-do), instead of the
  top-most tier.

### Added — developer-experience tooling (no wire change)
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
