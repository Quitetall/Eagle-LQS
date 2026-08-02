"""Public Python API for reusable Eagle claim verification."""

from .claims import ClaimResult, MissingEvidenceError, VerifyClaimsResult, verify_claims

__all__ = [
    "ClaimResult",
    "MissingEvidenceError",
    "VerifyClaimsResult",
    "verify_claims",
]
