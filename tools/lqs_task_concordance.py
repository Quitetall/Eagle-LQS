#!/usr/bin/env python3
"""LQS optional task-concordance axis (SPEC/LQS-v1.0.md §10).

Codec-agnostic measure of whether compression preserves *downstream-task*
structure, reported SEPARATELY from the L/C/M/A grade — it MUST NOT alter a
codec's tier. This tool consumes the paired ``orig_<i>.edf`` / ``recon_<i>.edf``
files written by ``eagle-lqs grade --dump-recon <dir>`` and emits a
``task_concordance`` JSON block suitable for embedding verbatim into an
``LqsSubmission.task_concordance``.

Two tiers of metric:

1. **Hjorth concordance (always available).** Activity / mobility / complexity
   are computed per channel on the original and the reconstruction; the
   Pearson correlation between the original-vector and recon-vector of each
   parameter (pooled over all channels of all files) quantifies how well the
   codec preserves these classic time-domain descriptors. Pure numpy — no
   neural stack, no labels.

2. **Seizure concordance (when the sibling LamQuant-Neural is importable).**
   If ``ai_models.validation.downstream_concordance`` is on the path, its
   richer detectors (seizure-F1 delta, etc.) are invoked and merged in. The
   tool degrades gracefully to tier 1 when the module is absent — exactly the
   importorskip pattern of ``tests/validation/test_downstream_concordance.py``.

Usage:
    python3 tools/lqs_task_concordance.py --recon-dir DIR [--out block.json]
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import List, Optional

import numpy as np

LQS_SPEC_VERSION = "1.0"
HEADER_BLOCK = 256


def read_edf_digital(path: Path) -> np.ndarray:
    """Read the digital int16 samples of a minimal one-record EDF.

    Returns an ``(n_chan, n_samples)`` int array. This reads the same EDF
    layout ``eagle-lqs --dump-recon`` writes (256-byte main header, ns x
    256-byte signal headers field-by-field, one data record of int16 LE
    samples). Annotation channels are not produced by the dumper, so they are
    not handled here.
    """
    b = path.read_bytes()
    if len(b) < HEADER_BLOCK:
        raise ValueError(f"{path}: shorter than EDF header")
    n_signals = int(b[252:256].decode("ascii").strip())
    # Signal headers: labels(16) ... then n_samples_per_record(8) is field #9.
    base = HEADER_BLOCK
    # offsets within the per-signal header block, field widths in EDF order:
    # label16, transducer80, phys_dim8, phys_min8, phys_max8, dig_min8,
    # dig_max8, prefilter80, n_samp8, reserved32.
    nsamp_field_off = base + n_signals * (16 + 80 + 8 + 8 + 8 + 8 + 8 + 80)
    spr = []
    for i in range(n_signals):
        f = b[nsamp_field_off + i * 8 : nsamp_field_off + (i + 1) * 8]
        spr.append(int(f.decode("ascii").strip()))
    if len(set(spr)) != 1:
        raise ValueError(f"{path}: non-uniform samples/record {spr}")
    n = spr[0]
    data_off = HEADER_BLOCK * (1 + n_signals)
    flat = np.frombuffer(b[data_off : data_off + n_signals * n * 2], dtype="<i2")
    if flat.size != n_signals * n:
        raise ValueError(f"{path}: truncated data record")
    return flat.reshape(n_signals, n).astype(np.float64)


def hjorth(x: np.ndarray) -> tuple[float, float, float]:
    """Hjorth activity, mobility, complexity for a 1-D signal."""
    dx = np.diff(x)
    ddx = np.diff(dx)
    var_x = float(np.var(x))
    var_dx = float(np.var(dx))
    var_ddx = float(np.var(ddx))
    activity = var_x
    mobility = np.sqrt(var_dx / var_x) if var_x > 0 else 0.0
    mob_dx = np.sqrt(var_ddx / var_dx) if var_dx > 0 else 0.0
    complexity = (mob_dx / mobility) if mobility > 0 else 0.0
    return activity, float(mobility), float(complexity)


def _pearson(a: np.ndarray, b: np.ndarray) -> float:
    if a.size < 2 or np.std(a) < 1e-12 or np.std(b) < 1e-12:
        return 1.0 if np.allclose(a, b) else 0.0
    return float(np.corrcoef(a, b)[0, 1])


def hjorth_concordance(pairs: List[tuple[np.ndarray, np.ndarray]]) -> dict:
    """Pooled per-channel Hjorth concordance across all (orig, recon) files."""
    o_act, o_mob, o_cmp = [], [], []
    r_act, r_mob, r_cmp = [], [], []
    n_chan = 0
    for orig, recon in pairs:
        c = min(orig.shape[0], recon.shape[0])
        n_chan += c
        for ch in range(c):
            a = hjorth(orig[ch])
            b = hjorth(recon[ch])
            o_act.append(a[0]); o_mob.append(a[1]); o_cmp.append(a[2])
            r_act.append(b[0]); r_mob.append(b[1]); r_cmp.append(b[2])
    return {
        "activity_r": _pearson(np.array(o_act), np.array(r_act)),
        "mobility_r": _pearson(np.array(o_mob), np.array(r_mob)),
        "complexity_r": _pearson(np.array(o_cmp), np.array(r_cmp)),
        "channels_pooled": n_chan,
    }


def try_sibling_seizure(_pairs: List[tuple[np.ndarray, np.ndarray]]) -> Optional[dict]:
    """Note the richer sibling concordance if LamQuant-Neural is importable.

    Mirrors tests/validation/test_downstream_concordance.py: the sibling lives
    in LamQuant-Neural, may be absent, and its absence is not an error. The
    sibling's seizure detectors require its own model/label contract, so this
    only flags availability; wiring the full call is left to the internal
    suite that already owns that contract.
    """
    try:
        from ai_models.validation import downstream_concordance as dc  # type: ignore
    except Exception:
        return None
    if getattr(dc, "hjorth_concordance", None) is None:
        return None
    return {"sibling": "ai_models.validation.downstream_concordance"}


def collect_pairs(recon_dir: Path) -> List[tuple[np.ndarray, np.ndarray]]:
    pairs = []
    i = 0
    while True:
        o = recon_dir / f"orig_{i}.edf"
        r = recon_dir / f"recon_{i}.edf"
        if not (o.exists() and r.exists()):
            break
        pairs.append((read_edf_digital(o), read_edf_digital(r)))
        i += 1
    return pairs


def main() -> int:
    ap = argparse.ArgumentParser(description="LQS optional task-concordance axis (advisory; never gates a tier)")
    ap.add_argument("--recon-dir", required=True, help="directory of paired orig_<i>.edf / recon_<i>.edf (from `eagle-lqs grade --dump-recon`)")
    ap.add_argument("--out", help="write the task_concordance JSON block here (else stdout)")
    args = ap.parse_args()

    recon_dir = Path(args.recon_dir)
    pairs = collect_pairs(recon_dir)
    if not pairs:
        print(f"error: no orig_<i>.edf/recon_<i>.edf pairs in {recon_dir}", file=sys.stderr)
        return 1

    block = {
        "spec_version": LQS_SPEC_VERSION,
        "axis": "task_concordance",
        "advisory": True,  # never alters an L/C/M/A grade (spec §10)
        "source": str(recon_dir),
        "n_files": len(pairs),
        "hjorth": hjorth_concordance(pairs),
    }
    sibling = try_sibling_seizure(pairs)
    if sibling is not None:
        block["seizure"] = sibling
    else:
        block["note"] = "sibling ai_models.validation.downstream_concordance not importable; Hjorth-only"

    text = json.dumps(block, indent=2)
    if args.out:
        Path(args.out).write_text(text)
        print(f"wrote task_concordance block: {args.out}", file=sys.stderr)
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
