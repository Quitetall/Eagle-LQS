"""Fail-closed policy for Eagle's mandatory offline test lane."""
from __future__ import annotations

import pytest


def pytest_sessionfinish(session, exitstatus) -> None:
    reporter = session.config.pluginmanager.get_plugin("terminalreporter")
    skipped = reporter.stats.get("skipped", []) if reporter is not None else []
    if skipped:
        if reporter is not None:
            reporter.write_sep(
                "=",
                f"FAIL: mandatory Eagle lane recorded {len(skipped)} skipped test(s)",
            )
        if exitstatus == pytest.ExitCode.OK:
            session.exitstatus = pytest.ExitCode.TESTS_FAILED
