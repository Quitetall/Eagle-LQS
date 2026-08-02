from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from eagle.claims import VerifyClaimsResult, verify_claims


def _make_evidence_dir(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "chbmit_lpc_mode_compare.json").write_text(
        json.dumps({
            "results": [
                {
                    "mode": "adaptive",
                    "files_in": 686,
                    "input_bytes": 45756994304,
                    "output_bytes": 16804264365,
                    "cr": 2.7229,
                }
            ]
        }),
        encoding="utf-8",
    )
    (root / "gzip_baseline_000.json").write_text(
        json.dumps(
            {
                "files": 453,
                "cr": 1.6033,
                "input_bytes": 6010000000,
                "output_bytes": 3750000000,
                "wall_seconds": 271,
            }
        ),
        encoding="utf-8",
    )
    (root / "tueg_subset_breakdown_montage.json").write_text(
        json.dumps(
            {
                "tree": ["v2.0.2"],
                "files_total": 70831,
                "groups": {
                    "01_tcp_ar": {
                        "input_bytes": int(round(1373.05 * 1e9)),
                        "output_bytes": int(round(603.11 * 1e9)),
                        "cr": 2.277,
                    },
                    "02_tcp_le": {
                        "input_bytes": int(round(249.38 * 1e9)),
                        "output_bytes": int(round(109.64 * 1e9)),
                        "cr": 2.274,
                    },
                    "03_tcp_ar_a": {
                        "input_bytes": int(round(133.70 * 1e9)),
                        "output_bytes": int(round(55.19 * 1e9)),
                        "cr": 2.422,
                    },
                    "04_tcp_le_a": {
                        "input_bytes": int(round(0.2173 * 1e9)),
                        "output_bytes": int(round(0.0937 * 1e9)),
                        "cr": 2.318,
                    },
                    "other": {
                        "input_bytes": 8290458,
                        "output_bytes": 9819030,
                        "cr": 1.0,
                    },
                },
            }
        ),
        encoding="utf-8",
    )
    for corpus, h_raw, h_diff, cr_raw, cr_diff in [
        ("tuar", 9.862, 8.327, 1.622, 1.922),
        ("tueg", 10.775, 9.337, 1.485, 1.714),
        ("tuev", 7.063, 5.620, 2.265, 2.847),
        ("tusl", 10.402, 8.327, 1.538, 1.922),
        ("tusz", 10.412, 8.865, 1.537, 1.805),
        ("chbmit", 8.387, 6.724, 1.908, 2.379),
    ]:
        (root / f"shannon_entropy_{corpus}.json").write_text(
            json.dumps(
                {
                    "H_raw": h_raw,
                    "H_diff": h_diff,
                    "cr_raw_ceiling": cr_raw,
                    "cr_diff_ceiling": cr_diff,
                }
            ),
            encoding="utf-8",
        ),
        encoding="utf-8",
    (root / "shannon_entropy_full_summary.json").write_text(
        json.dumps(
            {
                "aggregate": {
                    "H_raw_weighted": 9.244,
                    "H_diff_weighted": 7.656,
                    "cr_raw_ceiling": 1.731,
                    "cr_diff_ceiling": 2.090,
                }
            }
        )
    )


def test_importable_api() -> None:
    assert VerifyClaimsResult
    sample = VerifyClaimsResult(passes=0, failures=0, total=0)
    assert sample.exit_code == 0
    assert sample.total == 0
    # verify_claims function is importable and callable.
    assert callable(verify_claims)


def test_verify_claims_help() -> None:
    result = subprocess.run(
        [sys.executable, "tools/verify_paper_claims.py", "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0
    assert "evidence-dir" in (result.stdout + result.stderr)


def test_verify_claims_explicit_evidence_directory(tmp_path: Path) -> None:
    evidence_dir = tmp_path / "evidence"
    _make_evidence_dir(evidence_dir)
    result = subprocess.run(
        [
            sys.executable,
            "tools/verify_paper_claims.py",
            "--evidence-dir",
            str(evidence_dir),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0
    assert "[PASS]" in result.stdout
    assert "=== 60 PASS / 0 FAIL ===" in result.stdout


def test_verify_claims_missing_evidence(tmp_path: Path) -> None:
    result = subprocess.run(
        [
            sys.executable,
            "tools/verify_paper_claims.py",
            "--evidence-dir",
            str(tmp_path / "missing"),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode != 0
    assert "missing evidence" in result.stderr.lower()
    assert "Traceback" not in result.stderr


def test_verify_claims_preserves_failure_output_contract(tmp_path: Path) -> None:
    evidence_dir = tmp_path / "evidence"
    _make_evidence_dir(evidence_dir)
    claim_path = evidence_dir / "chbmit_lpc_mode_compare.json"
    claim = json.loads(claim_path.read_text(encoding="utf-8"))
    claim["results"][0]["files_in"] = 685
    claim_path.write_text(json.dumps(claim), encoding="utf-8")

    result = subprocess.run(
        [
            sys.executable,
            "tools/verify_paper_claims.py",
            "--evidence-dir",
            str(evidence_dir),
        ],
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 1
    assert "[FAIL] CHB-MIT files=686" in result.stdout
    assert "=== 59 PASS / 1 FAIL ===" in result.stdout
    assert "FAILURES:" in result.stdout
