# Eagle

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20484969.svg)](https://doi.org/10.5281/zenodo.20484969)

LamQuant's validation and benchmarking suite. Reproduces every numerical claim in the *IEEE Journal of Biomedical and Health Informatics* paper "LamQuant Lossless: A Real-Time, Bit-Exact, Wirelessly-Deployable EEG Compression Algorithm" (2026 submission), archived at Zenodo [`10.5281/zenodo.20484969`](https://doi.org/10.5281/zenodo.20484969).

**API reference:** [API.md](API.md)

## Headline numbers

| Metric | Value |
|--------|-------|
| TUEG v2.0.2 compression ratio (1.76 TB, 70,831 files) | **2.287:1** |
| CHB-MIT compression ratio | **2.723:1** (+15.9% vs Chen et al.) |
| RP2350 Hazard3 throughput (RISC-V, Verilator-measured, 150 MHz) | 0.627 Msa/s — **119× real-time**, CPI 1.071 |
| Bit-exact round-trip verification | 88,147 encode/decode operations across 13 corpora — zero failures |

Full evidence: `evidence/*.json`. Verified end-to-end by `tools/verify_paper_claims.py`.

## Repository layout

| Path | Purpose |
|------|---------|
| `eagle/` | Rust crate — LamQuant codec adapter + LQS fast gate |
| `lqs/` | Rust crate — LamQuant Quality Suite (wraps [OpenECS](https://github.com/Quitetall/OpenECS)) |
| `tools/bench_chbmit.py` | CHB-MIT CR + latency bench (paper Table II / IV) |
| `tools/bench_tueg_subsets.py` | Per-corpus TUEG breakdown (paper Appendix A) |
| `tools/bench_per_file_cr.py` | Per-file CR distribution + boxplot data (paper Figure 3) |
| `tools/bench_shannon_entropy.py` | Empirical entropy ceiling H₀, H₀(ΔX) (paper §II.D) |
| `tools/bench_edf_reader_parity.py` | MNE / pyedflib cross-validation (paper §IV.B) |
| `tools/bench_moabb_concordance.py` | BCI motor-imagery decoding concordance |
| `tools/bench_gzip_baseline.sh` | gzip / zstd baseline on TUEG sample |
| `tools/verify_paper_claims.py` | Verifies every paper number against `evidence/*.json` |
| `tools/hazard3_bench/` | `no_std` Rust bench harness — RP2350 Hazard3 Verilator RTL (paper §IV.D) |
| `tools/bench/bench_rs/` | Host-side Criterion benches (codec hot paths) |
| `tests/benchmarks/` | CR + ablation + biological-fidelity + C-parity tests |

## Dependencies

Eagle depends on **[LamQuant-Lossless](https://github.com/Quitetall/LamQuant-Lossless)** (the codec under test). It is a standalone clone; no sibling layout is required. The Rust bench crates pull `lamquant-firmware` over a pinned git revision.

```bash
git clone git@github.com:Quitetall/Eagle-LQS.git
cd Eagle-LQS

# Python tools
pip install "lamquant-codec @ git+https://github.com/Quitetall/LamQuant-Lossless.git#subdirectory=reference_implementations/python_codec"
pip install -e .

# Rust: RP2350 firmware bench (riscv32 target)
cd tools/hazard3_bench && cargo build --target riscv32imac-unknown-none-elf

# Rust: host-side Criterion benches
cd tools/bench/bench_rs && cargo build
```

The firmware bench crates pin `lamquant-firmware` to a specific commit:

```toml
lamquant-firmware = { git = "https://github.com/Quitetall/LamQuant-Lossless.git", rev = "abcae4794c38b3d3e75d3c214063cf0307e3daba", default-features = false }
```

Bump `rev` to re-pin against a newer LamQuant-Lossless release.

## Reproducing paper numbers

```bash
# CHB-MIT bench (Table II / IV)
python3 tools/bench_chbmit.py --corpus /path/to/chbmit --out evidence/

# TUEG subset bench — requires 1.76 TB local mirror (Appendix A)
python3 tools/bench_tueg_subsets.py --corpus /path/to/tueg --out evidence/

# RP2350 cycle count — requires Verilator (paper §IV.D)
cd tools/hazard3_bench && bash run_bench.sh verilator

# Verify all paper claims against evidence JSON
python3 tools/verify_paper_claims.py
# Expected output: 60 PASS / 0 FAIL
```

## Test suite

Eagle's tests are partitioned into two tiers by pytest marker.

**External (default).** Codec-agnostic: the `lqs` crate and agnostic Python
suites treat the codec as an opaque compress / decompress binary and verify
only externally-observable quantities — compression ratio, bit-exact round-trip,
latency, EDF byte-level parity. No neural stack required.

```bash
pytest -m "not internal"   # external Python suite
cargo test -p eagle        # Rust fast gate (lqs + LamQuantLossless adapter)
```

**Internal (LamQuant vendor).** Reaches inside neural codec internals — FSQ
entropy, latent utilization, Cayley rotation, residual-FSQ, subband leakage,
TNN memory, XNOR / cpop MACs, C-vs-Python parity, ablation matrix. Requires
the sibling **LamQuant-Neural** source tree and the **LamQuant-Lossless** wheel.

```bash
pip install -e '.[neural]'
pytest -m internal
```

**BCI downstream concordance.** `tools/bench_moabb_concordance.py` measures
whether a codec preserves downstream motor-imagery decoding accuracy by running
compress → decompress → CSP+LDA decode → compare. The codec is reached through
the `lml` binary (`$LML_BIN` → sibling build → `$PATH`), so this bench belongs
to the external tier and requires no neural stack.

```bash
# Synthetic offline (no corpus download):
LML_BIN=/path/to/lml pytest -m bci tests/validation/test_moabb_concordance.py

# Real motor-imagery data (BNCI2014_001, etc.):
pip install -e '.[moabb]'
python3 tools/bench_moabb_concordance.py --source moabb \
    --dataset BNCI2014_001 --paradigm LeftRightImagery --subject 1
```

## Rust fast gate

The `eagle` crate and the in-repo `lqs` crate form the Rust fast gate — the
same check CI runs before any merge. To mirror it locally before every push:

```bash
chmod +x scripts/lqs-fastgate.sh .githooks/pre-push
git config core.hooksPath .githooks
```

`scripts/lqs-fastgate.sh` runs `cargo build -q -p eagle && cargo test -q -p eagle`. Run it standalone at any time.

## Architecture

| Public | Private (for now) |
|--------|------------------|
| LamQuant-Lossless (codec under test) | LamQuant (monorepo source of truth) |
| **Eagle-LQS** (this repo — validation) | LamQuant-Neural (SNN/TNN models) |
| [OpenECS](https://github.com/Quitetall/OpenECS) (vendor-neutral EEG codec standard) | LamQuant-Codec (turnkey integration) |
| BLUT (training orchestrator) | |
| LamQuant-Vision (LSL + viz) | |

## License

GNU Affero General Public License v3 — see `LICENSE.md`.

## Citation

```bibtex
@article{lam2026lamquant,
  title   = {LamQuant Lossless: A Real-Time, Bit-Exact, Wirelessly-Deployable EEG Compression Algorithm},
  author  = {Lam, Brian},
  journal = {IEEE Journal of Biomedical and Health Informatics},
  year    = {2026},
  note    = {Submitted}
}
```
