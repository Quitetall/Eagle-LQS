# ADR 0056: Eagle as a BLUT cookbook (testing/validation/benchmarking framework)

## Status

`Proposed 2026-06-20`

## Context

LamQuant has three evaluation/testing components with no unified execution model:

1. **Eagle** (`evaluation/eagle-lqs/eagle/`) — 740 lines Rust. A single LamQuant adapter (`LamQuantLossless`, shells to `lml` binary) + a CLI that grades via LQS harness. Not a framework — a module.
2. **LQS** (`evaluation/eagle-lqs/lqs/`) — 8 lines Rust. Thin re-export of OpenECS primitives (`adapter`, `edf`, `harness`).
3. **OpenECS** (`evaluation/openecs/`) — standalone Rust crate. Vendor-neutral EEG codec standard: `Codec` trait, `harness::run()`, metrics (PRD/PRDN/R/SNR), levels (L/C/M/A grading), bands, reports, corpus management. 131 tests.

Python test suites exist in `evaluation/eagle-lqs/tests/` (27 benchmark modules, 7 validation modules, 1 audit module, 6 helpers, 2 fixture files) but have no connection to the Rust execution model.

Meanwhile, BLUT (`training/engine/`) already provides:
- **Recipe trait** — `NAME`, `DESCRIPTION`, `CATEGORY` (Course enum: DataPrep, Pretrain, Train, Eval, Gate, Export, Pipeline, User), `INPUT_KINDS`, `OUTPUT_KIND`, typed `Args` (serde + JsonSchema), `compile() → Plan`.
- **Stage trait** — typed `Input`/`Output` artifacts, async `run()`, `StageContext` (job_dir, status, cancellation, cache).
- **Plan** — DAG of stages, compiled and executed by the executor.
- **Lineage** — full history of recipe runs, inputs, outputs, errors.
- **Resume/retry** — re-run only failed stages.
- **Scheduling** — `blut schedule install` with systemd `OnCalendar`.
- **TUI** — Training Cockpit (hub-and-spoke, ADR 0053).
- **Admission/OOM guard** — resource-aware execution.
- **Artifact caching** — content-hash-based skip.

Two cookbooks already exist: `blut-lamquant` (6 recipes, 20+ stages) and `blut-lamu` (LamU recipes). Both follow the pattern: `register_recipe!` → `Recipe` impl → `Plan` of stages.

The parallel to training is exact. Training has:
```
BLUT engine → cookbooks (lamquant, lamu) → recipes → stages → artifacts
```

Testing should have:
```
BLUT engine → cookbooks (eagle) → recipes → stages → artifacts
```

A test is a pipeline: `FixtureSource → Encode → Decode → MeasureRoundtrip → CheckInvariants → Report`. Each step is a stage. The plan chains them. BLUT runs the DAG.

### Error handling gap

Currently every cookbook invents its own error handling. BLUT should own the error taxonomy the same way it owns the `Artifact` trait. A cookbook maps its domain errors to BLUT's error system:

- BLUT defines: `ErrorCode`, `StageFailure`, `Severity`, `ErrorDomain` trait
- Cookbooks register error domains: `register_error_domain!(EagleErrorDomain)`
- Stage failures carry structured context (key-value pairs), not prose strings
- Lineage stores failures natively, not as log strings

### TUI separation

Per ADR 0053 (hub-and-spoke), each binary is a standalone tool with its own TUI. BLUT's TUI is the Training Cockpit. Eagle's TUI is a separate spoke binary (`eagle-tui`) that calls the Eagle cookbook. The hub (`lamquant`) detects and launches both. Eagle's TUI does not embed BLUT's TUI — it registers with the hub as a spoke.

## Decision

Eagle becomes a BLUT cookbook. The current `eagle` crate (740 lines, single adapter + CLI) is refactored into a cookbook crate under `evaluation/cookbooks/eagle/`. BLUT gains first-class error infrastructure. Eagle's TUI is a separate spoke binary.

### Directory structure

```
evaluation/
├── cookbooks/
│   └── eagle/                          ← NEW: BLUT cookbook
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs                  ← register recipes + error domain
│           ├── errors.rs               ← Eagle error codes (E_ROUNDTRIP, E_REJECT, etc.)
│           ├── artifacts/
│           │   ├── mod.rs
│           │   ├── test_signal.rs      ← TestSignal { channels, fs }
│           │   ├── encoded_blob.rs     ← EncodedBlob { bytes, metadata }
│           │   ├── roundtrip_result.rs ← RoundtripResult { passed, prd, r }
│           │   └── audit_report.rs     ← AuditReport { invariants, failures, coverage }
│           ├── stages/
│           │   ├── mod.rs
│           │   ├── fixture_source.rs   ← load/generate test fixtures
│           │   ├── encode.rs           ← run codec.encode()
│           │   ├── decode.rs           ← run codec.decode()
│           │   ├── check_roundtrip.rs  ← assert original == decoded
│           │   ├── check_reject.rs     ← assert invalid input is rejected
│           │   ├── check_threshold.rs  ← assert metric >= floor
│           │   ├── check_parity.rs     ← assert firmware == desktop
│           │   ├── grade_lqs.rs        ← run LQS harness, check grade
│           │   └── generate_report.rs  ← compile results → AuditReport
│           └── recipes/
│               ├── mod.rs
│               ├── eagle_codec.rs      ← lossless codec test recipe
│               ├── eagle_lqs.rs        ← LQS grading recipe
│               └── eagle_openecs.rs    ← OpenECS compliance recipe
│
├── eagle-tui/                          ← NEW: spoke binary (ADR 0053)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                     ← registers with lamquant hub
│
├── eagle-lqs/                          ← STAYS: LQS crate (Eagle depends on it)
│   ├── lqs/                            ← vendor-neutral grading primitives
│   ├── tests/                          ← Python test suite (migrates under cookbook)
│   └── tools/                          ← bench tools, hazard3 bench
│
└── openecs/                            ← STAYS: OpenECS crate (Eagle depends on it)
```

### Error infrastructure in BLUT

New module in `training/engine/src/framework/error_domain.rs`:

```rust
/// Trait for cookbooks to register their domain-specific error codes.
pub trait ErrorDomain: Send + Sync + 'static {
    const NAME: &'static str;
    const CODES: &[(&'static str, &'static str)];  // (code, description)
}

/// Structured failure context. Stored in lineage.
pub struct StageFailure {
    pub code: ErrorCode,
    pub domain: &'static str,       // "eagle", "lamquant", "lamu"
    pub stage: &'static str,
    pub recipe: &'static str,
    pub context: Vec<(&'static str, String)>,
    pub severity: Severity,
    pub file: &'static str,
    pub line: u32,
}

pub enum Severity {
    Critical,  // data loss, corruption, safety violation
    Major,     // wrong output, failed invariant
    Minor,     // perf regression, edge case
}
```

Eagle registers its error domain:

```rust
// evaluation/cookbooks/eagle/src/errors.rs
pub mod eagle_codes {
    use blut::framework::error::ErrorCode;
    pub const E_ROUNDTRIP: ErrorCode = ErrorCode::Custom("EAGLE_ROUNDTRIP");
    pub const E_REJECT: ErrorCode = ErrorCode::Custom("EAGLE_REJECT");
    pub const E_THRESHOLD: ErrorCode = ErrorCode::Custom("EAGLE_THRESHOLD");
    pub const E_PARITY: ErrorCode = ErrorCode::Custom("EAGLE_PARITY");
    pub const E_FORMAT: ErrorCode = ErrorCode::Custom("EAGLE_FORMAT");
}
```

### Recipe structure

Each Eagle recipe is a BLUT `Recipe` impl that compiles to a `Plan` of stages:

```rust
// evaluation/cookbooks/eagle/src/recipes/eagle_codec.rs

#[derive(Default)]
pub struct EagleCodec;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Args {
    pub lml_bin: String,
    pub channel_counts: Vec<usize>,
    pub signal_length: usize,
    pub sample_rate: f64,
    pub check_truncated: bool,
    pub check_parity: bool,
}

impl Recipe for EagleCodec {
    const NAME: &'static str = "eagle-codec";
    const DESCRIPTION: &'static str = "Lossless codec test suite";
    const CATEGORY: RecipeCategory = RecipeCategory::Eval;
    const OUTPUT_KIND: &'static str = "audit_report.json";
    type Backend = PythonBackend;
    type Args = Args;

    fn compile(&self, args: Args) -> Result<Plan<(), Self::Backend>> {
        // Build DAG: fixtures → encode → decode → check → report
    }
}

blut::register_recipe!(EagleCodec);
```

### TUI separation

Eagle's TUI is a spoke binary (`evaluation/eagle-tui/`) that:
- Depends on `lamquant-tui` (shared framework, ADR 0053)
- Registers its own panels (test dashboard, invariant view, error log)
- Calls `blut recipe run eagle-*` via the BLUT engine
- Is detected and launched by the `lamquant` hub

BLUT's TUI (Training Cockpit) does not change. It shows training recipes. Eagle's TUI shows testing recipes. Both are spokes of the same hub.

## Rationale

- **Testing is a pipeline.** `FixtureSource → Encode → Decode → Check → Report` is a DAG. BLUT executes DAGs. Building a separate orchestrator for testing duplicates BLUT's core competency.
- **Lineage for free.** Test history (what passed, when, on what commit, with what errors) is stored in BLUT's lineage system. No separate DB.
- **Scheduling for free.** `blut schedule install eagle-codec --calendar "03:00"` runs nightly tests. No cron integration needed.
- **Resume for free.** Failed test run? `blut recipe run eagle-codec` resumes from the failed stage, skips passed ones.
- **Error infrastructure shared.** Training failures and test failures use the same structured error system. One `blut errors list` command shows all domains.
- **TUI separation preserves focus.** Eagle's TUI shows test results. BLUT's TUI shows training jobs. Neither pollutes the other. Both are spokes of the hub (ADR 0053).

## Alternatives Considered

- **Standalone Eagle framework** — build a separate orchestrator (error codes, manifest, report generator, CLI, TUI). ~2000 lines of new code vs ~500 for a cookbook. Rejects: duplicates BLUT's DAG executor, lineage, scheduling, resume, admission guard. The only thing it adds is independence from BLUT, which is not a requirement — Eagle is internal tooling, not a library for external consumers.
- **Eagle as a Rust library + pytest plugin** — no orchestrator, just assertion helpers that tests call directly. Simpler, but loses lineage, scheduling, resume, and the audit surface. Every test file is a snowflake. No single-command "run all tests and generate report".
- **Cranelift JIT for BLUT** — JIT-compile stage chains for fast test iteration. Cranelift compiles IR → native code; BLUT's overhead is async dispatch + artifact serialization, not code generation. The real optimization is `--only` (filter to specific stages) + caching (skip unchanged stages) + ParallelExecutor (run independent stages concurrently). Cranelift is the wrong tool.

## Consequences

- **Dead code:** The current `eagle` crate (740 lines) is refactored into the cookbook. `eagle/src/adapters_lamquant.rs` moves to a stage. `eagle/src/bin/eagle.rs` becomes `blut recipe run eagle-codec`.
- **New dependency:** Eagle cookbook depends on `blut` (the engine crate). This is the same dependency `blut-lamquant` and `blut-lamu` have.
- **BLUT gains error infra:** `blut-core` adds `ErrorDomain` trait, `StageFailure`, `Severity`. All cookbooks benefit. Backwards-compatible (existing stages that return `String` errors still work).
- **TUI split:** Eagle's TUI is a new spoke binary. BLUT's TUI is unchanged. Hub detects both.
- **Python tests migrate:** The Python test suite (`evaluation/eagle-lqs/tests/`) moves under the cookbook. Pytest still runs them; BLUT orchestrates the Rust stages around them.
- **LQS and OpenECS stay standalone.** They are dependency crates, not cookbooks. Eagle depends on them.

## Implementation Plan

- **Deliverable:** `evaluation/cookbooks/eagle/` — a BLUT cookbook with 3 recipes (eagle-codec, eagle-lqs, eagle-openecs), error domain registration, and 9 stages.
- **Acceptance gate (fail-CLOSED; unmeasured ⇒ FAIL):**
  - `gate_cmd:` `cd evaluation/cookbooks/eagle && cargo test && blut recipe run eagle-codec --args '{"lml_bin":"lml","channel_counts":[4],"signal_length":4096,"sample_rate":256.0,"check_truncated":false,"check_parity":false}'`
  - `pass when:` exit==0, all stages complete, AuditReport produced with no Critical/Major failures
- **Rollback:** delete `evaluation/cookbooks/eagle/`, restore `evaluation/eagle-lqs/eagle/` from git.

## Progress Log

- _2026-06-20 — opened._

## Related Decisions

- ADR 0034 — BLUT scope charter (BLUT = domain control plane)
- ADR 0046 — BLUT resource broker (admission gate)
- ADR 0051 — Ingredient taxonomy and registry (BLUT → Cookbook → Course → Recipe → Ingredient)
- ADR 0053 — TUI hub-and-spoke architecture (standalone binaries, hub as launcher)
- ADR 0055 — Ingredient taxonomy and registry

## Validation

This decision is wrong if:
- BLUT's dependency is too heavy for "just run tests" (measure: `blut recipe run` overhead > 2s for a trivial recipe)
- The TUI split creates confusion about which tool to use (measure: user feedback)
- The error infrastructure doesn't get adopted by other cookbooks (measure: `blut errors list` shows only eagle domain after 30 days)
