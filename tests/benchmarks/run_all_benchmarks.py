#!/usr/bin/env python3
"""
LamQuant Gen 6 — Benchmark Gauntlet (FIXED)
============================================
Runs all 4 audits in sequence. Each must pass independently.
"""
import sys
import os
import time
import numpy as np

# Ensure sibling benchmark modules are importable by bare name
sys.path.insert(0, os.path.dirname(__file__))

import benchmark_tnn_memory
import benchmark_biological_fidelity
import benchmark_c_parity


def render_banner(title):
    print("\n" + "=" * 80)
    print(f" {title.center(78)} ")
    print("=" * 80)


def render_result(item, status="PASS"):
    pad = max(1, 65 - len(item))
    colors = {
        "PASS": "\033[92m",  # green
        "FAIL": "\033[91m",  # red
        "SKIP": "\033[93m",  # yellow
    }
    color = colors.get(status, "\033[91m")
    reset = "\033[0m"
    print(f"[*] {item}" + "." * pad + f"[{color}{status}{reset}]")


def main():
    print("\n" + "x" * 80)
    print(" LAMQUANT GEN 6 : BENCHMARK GAUNTLET ".center(80))
    print("x" * 80)

    # 1. MEMORY
    render_banner("AUDIT 1: TNN MEMORY FOOTPRINT")
    try:
        result = benchmark_tnn_memory.run()
        if result is None:
            render_result("TNN Memory (missing ckpt)", "SKIP")
        else:
            used, budget = result
            render_result(f"TNN: {used} / {budget} bytes")
    except SystemExit:
        render_result("TNN Memory", "FAIL")
    except Exception as e:
        print(f"\n[!!!] MEMORY BENCHMARK CRASHED: {e}")
        render_result("TNN Memory", "FAIL")

    time.sleep(0.3)

    # 2. BIOLOGICAL FIDELITY
    render_banner("AUDIT 2: BIOLOGICAL FIDELITY (Held-Out Patients)")
    try:
        result = benchmark_biological_fidelity.run()
        if result is None:
            render_result("Biological Fidelity (missing ckpt or patient data)", "SKIP")
        else:
            _, min_r = result
            render_result(f"Min R = {min_r:.4f} (floor: 0.85)")
    except SystemExit:
        render_result("Biological Fidelity", "FAIL")
    except Exception as e:
        print(f"\n[!!!] FIDELITY BENCHMARK CRASHED: {e}")
        render_result("Biological Fidelity", "FAIL")

    time.sleep(0.3)

    # 3. C PARITY
    render_banner("AUDIT 3: C-SIMULATION BIT PARITY")
    try:
        result = benchmark_c_parity.run()
        if result is None:
            render_result("C Parity (missing ckpt or firmware header)", "SKIP")
        else:
            render_result(f"Max cascaded drift: {result:.6f}")
    except SystemExit:
        render_result("C Parity", "FAIL")
    except Exception as e:
        print(f"\n[!!!] PARITY BENCHMARK CRASHED: {e}")
        render_result("C Parity", "FAIL")

    # 4. CLINICAL HARNESS (requires full dataset + teacher checkpoint)
    render_banner("AUDIT 4: CLINICAL STRESS PROFILES")
    try:
        # clinical_master_harness.py should be in the same directory as this script
        from clinical_master_harness import ClinicalMasterHarness, STRESS_PROFILES
        import torch
        harness = ClinicalMasterHarness(use_gpu=torch.cuda.is_available(), stdout=False)
        harness.run_suite()
        if harness.skipped:
            render_result(f"Clinical Harness ({harness.skip_reason})", "SKIP")
        else:
            pass_count = sum(1 for r in harness.full_results if r['passed'])
            total = len(harness.full_results)
            if pass_count == total:
                render_result(f"Clinical: {pass_count}/{total} profiles passed")
            else:
                render_result(f"Clinical: {pass_count}/{total} profiles passed", "FAIL")
    except ImportError as e:
        print(f"[!] Clinical harness import failed: {e}")
        print(f"[!] Ensure clinical_master_harness.py is in: {os.path.dirname(__file__)}")
        render_result("Clinical Harness (import failed)", "SKIP")
    except Exception as e:
        print(f"[!] Clinical harness error: {e}")
        import traceback
        traceback.print_exc()
        render_result("Clinical Harness (CRASHED)", "FAIL")

    # 5. DEPLOYMENT PATH (ternary encoder → FP32 decoder)
    render_banner("AUDIT 5: DEPLOYMENT PATH (Ternary Encoder → FP32 Decoder)")
    try:
        import benchmark_deployment_path
        result = benchmark_deployment_path.run()
        if result is None:
            render_result("Deployment Path (missing ckpt or patient data)", "SKIP")
        else:
            route_a_results, _ = result
            deploy_rs = [r['r'] for r in route_a_results]
            render_result(f"Deployment Min R = {min(deploy_rs):.4f} (floor: 0.85)")
    except SystemExit:
        render_result("Deployment Path", "FAIL")
    except ImportError as e:
        print(f"[!] Deployment benchmark import failed: {e}")
        render_result("Deployment Path (import failed)", "SKIP")
    except Exception as e:
        print(f"[!] Deployment benchmark error: {e}")
        import traceback
        traceback.print_exc()
        render_result("Deployment Path (CRASHED)", "FAIL")

    # 6. COMPRESSION RATIO (real FSQ + rANS byte count at multiple levels)
    render_banner("AUDIT 6: COMPRESSION RATIO (FSQ + rANS)")
    try:
        import benchmark_compression_ratio
        result = benchmark_compression_ratio.run()
        if result is None:
            render_result("Compression Ratio (missing ckpts or patient data)", "SKIP")
        else:
            # benchmark_compression_ratio.run() returns (golden_results_by_level, lightning_results)
            results_by_level = result[0] if isinstance(result, tuple) else result
            best = None
            for L in sorted(results_by_level.keys()):
                res = results_by_level[L]
                mcr = np.mean([r['cr'] for r in res])
                mr = np.mean([r['r'] for r in res])
                if mr > 0.95:
                    best = (L, mcr, mr)
                    break
            if best:
                L, mcr, mr = best
                render_result(f"Best: L={L}, CR={mcr:.1f}x, R={mr:.4f}")
            else:
                render_result("No level with R > 0.95", "FAIL")
    except SystemExit:
        render_result("Compression Ratio", "FAIL")
    except ImportError as e:
        print(f"[!] Compression ratio import failed: {e}")
        render_result("Compression Ratio (import failed)", "SKIP")
    except Exception as e:
        print(f"[!] Compression ratio error: {e}")
        import traceback
        traceback.print_exc()
        render_result("Compression Ratio (CRASHED)", "FAIL")

    print("\n" + "*" * 80)
    print(" GAUNTLET COMPLETE ".center(80))
    print("*" * 80 + "\n")


if __name__ == "__main__":
    main()
