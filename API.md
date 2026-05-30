# Eagle — API reference

> **WIP stub.** Surfaces are stabilizing during the repo→module refactor. This
> stub keeps the README link live; the full surface table lands once the bench
> harness layout settles.

**Owns:** the validation + benchmarking suite — reproduces every numerical claim in the
TBioCAS paper. Bench drivers (`verify_paper_claims.py`, `bench_chbmit.py`,
`bench_tueg_subsets.py`, …), the `hazard3_bench` RISC-V cycle harness, validation + audit
tests, bundled evidence JSON. Depends on `LamQuant-Lossless` (and optionally `LamQuant-Neural`).

To document next: `verify_paper_claims.py` invocation + PASS/FAIL contract, each bench
driver's input→output, the evidence-bundle layout, how to add a new benchmark.

The codec under test + `.lml`/`.lma` format is owned by `LamQuant-Lossless/API.md`.

See also: [README](README.md) · meta index [`../API.md`](../API.md).
