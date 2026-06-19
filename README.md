# Eagle-LQS

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20484969.svg)](https://doi.org/10.5281/zenodo.20484969)

LamQuant's validation and benchmarking suite. Reproduces all numerical claims in "LamQuant Lossless: A Real-Time, Bit-Exact, Wirelessly-Deployable EEG Compression Algorithm" (*IEEE JBHI*, 2026 submission), archived at Zenodo [`10.5281/zenodo.20484969`](https://doi.org/10.5281/zenodo.20484969).

**API reference:** [API.md](API.md)

## Benchmark results

| Metric | Result |
|--------|--------|
| TUEG v2.0.2 CR (1.76 TB, 70,831 files) | **2.287:1** |
| CHB-MIT CR | **2.723:1** (+15.9% vs Chen et al.) |
| RP2350 Hazard3 throughput (150 MHz, Verilator RTL sim) | 0.627 Msa/s, **119× real-time**, CPI 1.071 |
| RP2350 Hazard3 throughput (150 MHz, **silicon** — Pico 2, /dev/ttyACM0) | 0.60 Msa/s, **116× real-time**, CPI 1.074 |
| Bit-exact round-trip (13 corpora) | 88,147 operations, 0 failures |

Evidence JSON: `evidence/`. Claim verification: `tools/verify_paper_claims.py`.

## Repository layout

| Path | Purpose |
|------|---------|
| `eagle/` | Rust crate — `LamQuantLossless` codec adapter, `lqs` fast gate |
| `lqs/` | Rust crate — LamQuant Quality Suite (re-exports [OpenECS](https://github.com/Quitetall/OpenECS)) |
| `tools/bench_chbmit.py` | CHB-MIT CR + latency (paper Table II / IV) |
| `tools/bench_tueg_subsets.py` | TUEG per-corpus breakdown (paper Appendix A) |
| `tools/bench_per_file_cr.py` | Per-file CR distribution (paper Figure 3) |
| `tools/bench_shannon_entropy.py` | Empirical entropy H₀, H₀(ΔX) (paper §II.D) |
| `tools/bench_edf_reader_parity.py` | MNE / pyedflib EDF parity (paper §IV.B) |
| `tools/bench_moabb_concordance.py` | BCI downstream decoding concordance |
| `tools/bench_gzip_baseline.sh` | gzip / zstd reference baseline |
| `tools/verify_paper_claims.py` | Walks all paper numbers against `evidence/*.json` |
| `tools/hazard3_bench/` | `no_std` Rust bench → RP2350 Hazard3 Verilator RTL (paper §IV.D) |
| `tools/bench/bench_rs/` | Host-side Criterion benches (hot paths) |
| `tests/benchmarks/` | CR, ablation, biological-fidelity, C-parity |

## Setup

Depends on **[LamQuant-Lossless](https://github.com/Quitetall/LamQuant-Lossless)** (the codec under test). Standalone clone — no sibling layout required.

```bash
git clone git@github.com:Quitetall/Eagle-LQS.git && cd Eagle-LQS

# Python tools
pip install "lamquant-codec @ git+https://github.com/Quitetall/LamQuant-Lossless.git#subdirectory=reference_implementations/python_codec"
pip install -e .

# Rust: RP2350 firmware bench
cd tools/hazard3_bench && cargo build --target riscv32imac-unknown-none-elf

# Rust: host Criterion benches
cd tools/bench/bench_rs && cargo build
```

The firmware crates pin `lamquant-firmware` to a specific commit:

```toml
lamquant-firmware = { git = "https://github.com/Quitetall/LamQuant-Lossless.git", rev = "abcae4794c38b3d3e75d3c214063cf0307e3daba", default-features = false }
```

## Reproducing paper numbers

```bash
python3 tools/bench_chbmit.py --corpus /path/to/chbmit --out evidence/
python3 tools/bench_tueg_subsets.py --corpus /path/to/tueg --out evidence/   # 1.76 TB
cd tools/hazard3_bench && bash run_bench.sh verilator                         # Verilator required
python3 tools/verify_paper_claims.py                                          # 60 PASS / 0 FAIL
```

## Test suite

Tests are partitioned by pytest marker.

**External (default).** Codec-agnostic. Verifies compression ratio, bit-exact round-trip, latency, and EDF byte-level parity through the codec's encode/decode boundary only. No neural stack.

```bash
pytest -m "not internal"
cargo test -p eagle
```

**Internal.** LamQuant-specific introspection: FSQ entropy, latent utilization, Cayley rotation, residual-FSQ, subband leakage, TNN memory, XNOR/cpop MAC counts, C-vs-Python parity, ablation matrix. Requires LamQuant-Neural source and LamQuant-Lossless wheel.

```bash
pip install -e '.[neural]'
pytest -m internal
```

**BCI concordance.** `tools/bench_moabb_concordance.py` runs compress → decompress → CSP+LDA decode → compare and reports accuracy and Cohen's κ deltas. Reaches the codec through `$LML_BIN` → sibling build → `$PATH`. External tier — no neural stack.

```bash
LML_BIN=/path/to/lml pytest -m bci tests/validation/test_moabb_concordance.py

pip install -e '.[moabb]'
python3 tools/bench_moabb_concordance.py --source moabb \
    --dataset BNCI2014_001 --paradigm LeftRightImagery --subject 1
```

## Rust fast gate

```bash
chmod +x scripts/lqs-fastgate.sh .githooks/pre-push
git config core.hooksPath .githooks   # installs pre-push hook
```

`scripts/lqs-fastgate.sh` runs `cargo build -q -p eagle && cargo test -q -p eagle` — the same job as CI `eagle-rust`.

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
