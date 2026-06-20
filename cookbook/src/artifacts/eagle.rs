// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Eagle artifact types — data handles flowing between test stages.

use std::path::{Path, PathBuf};

use blut::framework::artifact::Artifact;
use blut::framework::ContentHash;
use serde::{Deserialize, Serialize};

// ── TestSignal ─────────────────────────────────────────────────────

/// A multi-channel integer signal (EEG digital domain, i64 samples).
/// Input to the encode stage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestSignal {
    pub channels: Vec<Vec<i64>>,
    pub fs: f64,
    pub content_hash: ContentHash,
}

impl TestSignal {
    pub fn new(channels: Vec<Vec<i64>>, fs: f64) -> Self {
        let content_hash = hash_signal(&channels);
        Self { channels, fs, content_hash }
    }
}

impl Artifact for TestSignal {
    const KIND: &'static str = "eagle.test_signal";
    const SCHEMA: u32 = 1;

    fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    fn primary_path(&self) -> &Path {
        Path::new("/dev/null") // in-memory artifact, no file
    }
}

// ── EncodedBlob ────────────────────────────────────────────────────

/// Compressed bytes produced by a codec encoder.
/// Input to the decode stage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodedBlob {
    pub bytes: Vec<u8>,
    pub codec_name: String,
    pub content_hash: ContentHash,
}

impl Artifact for EncodedBlob {
    const KIND: &'static str = "eagle.encoded_blob";
    const SCHEMA: u32 = 1;

    fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    fn primary_path(&self) -> &Path {
        Path::new("/dev/null") // in-memory artifact, no file
    }
}

// ── RoundtripResult ────────────────────────────────────────────────

/// Result of a lossless roundtrip check.
/// Produced by the decode stage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoundtripResult {
    /// Whether the decoded signal matches the original bit-exact.
    pub passed: bool,
    pub n_channels: usize,
    pub n_samples: usize,
    pub content_hash: ContentHash,
}

impl Artifact for RoundtripResult {
    const KIND: &'static str = "eagle.roundtrip_result";
    const SCHEMA: u32 = 1;

    fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    fn primary_path(&self) -> &Path {
        Path::new("/dev/null") // in-memory artifact, no file
    }
}

// ── AuditReport ────────────────────────────────────────────────────

/// Aggregated test report. Final output of an Eagle recipe.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub entries: Vec<RoundtripEntry>,
    pub path: PathBuf,
    pub content_hash: ContentHash,
}

/// One entry in the audit report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoundtripEntry {
    pub n_channels: usize,
    pub n_samples: usize,
    pub passed: bool,
}

impl Artifact for AuditReport {
    const KIND: &'static str = "eagle.audit_report";
    const SCHEMA: u32 = 1;

    fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    fn primary_path(&self) -> &Path {
        &self.path
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// SHA-256 over all signal samples (channels concatenated, i64 LE bytes).
pub fn hash_signal(channels: &[Vec<i64>]) -> ContentHash {
    let mut buf = Vec::new();
    for chan in channels {
        for &s in chan {
            buf.extend_from_slice(&s.to_le_bytes());
        }
    }
    ContentHash::of_bytes(&buf)
}

/// SHA-256 over raw bytes.
pub fn hash_bytes(bytes: &[u8]) -> ContentHash {
    ContentHash::of_bytes(bytes)
}
