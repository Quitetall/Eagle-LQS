"""Cross-language / cross-tree audit wrappers (pytest-callable).

Each test module wraps a `scripts/audit_*.py` invocation so the audit
gates run uniformly through `pytest tests/`. The audit scripts
themselves stay callable from the command line.

Audits sequestered as one-shots (need real data, not CI-shaped) live
under `legacy/one_shots/` and are NOT exposed here.
"""
