// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! Eagle backend — subprocess runner for the `lml` lossless codec binary.

use std::path::{Path, PathBuf};
use std::process::Command;

use blut::backends::TrainingBackend;

/// Default location of the prebuilt sibling `lml` binary.
const DEFAULT_LML_BIN: &str = "/tmp/lamquant-verify/LamQuant-Lossless/target/debug/lml";

/// Size in bytes of the fixed EDF main header and of each signal header.
const EDF_HEADER_BLOCK: usize = 256;

/// Eagle test backend identity.
pub struct EagleBackend;

impl TrainingBackend for EagleBackend {
    const ID: &'static str = "eagle";
    const DESCRIPTION: &'static str = "Eagle test backend (subprocess runner for lml)";
}

// ── Binary resolution ──────────────────────────────────────────────

/// Resolve the `lml` binary to invoke, or `None` if none is usable.
pub fn resolve_lml_bin() -> Option<PathBuf> {
    // 1. Explicit override via env var.
    if let Some(p) = std::env::var_os("LML_BIN") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    // 2. Prebuilt sibling default.
    let default = Path::new(DEFAULT_LML_BIN);
    if default.is_file() {
        return Some(default.to_path_buf());
    }
    // 3. `lml` on PATH.
    if Command::new("lml")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(PathBuf::from("lml"));
    }
    None
}

// ── EDF helpers ────────────────────────────────────────────────────

/// Write an ASCII value left-justified into a fixed-width EDF field,
/// space-padded. Returns an error if the value overflows the field.
fn edf_field(out: &mut Vec<u8>, value: &str, width: usize) -> Result<(), ()> {
    let bytes = value.as_bytes();
    if bytes.len() > width {
        return Err(());
    }
    out.extend_from_slice(bytes);
    out.resize(out.len() + (width - bytes.len()), b' ');
    Ok(())
}

/// Build a minimal, spec-valid EDF byte image for `signal` at rate `fs`.
///
/// One data record holds every channel's samples. Returns `None` when the
/// signal cannot be expressed as EDF digital samples: non-i16 values,
/// ragged channels, or an empty / zero-length signal.
pub fn write_edf_bytes(signal: &[Vec<i64>], fs: f64) -> Option<Vec<u8>> {
    let ns = signal.len();
    if ns == 0 {
        return None;
    }
    let spr: Vec<usize> = signal.iter().map(|c| c.len()).collect();
    if spr.contains(&0) {
        return None;
    }
    if signal
        .iter()
        .flat_map(|c| c.iter())
        .any(|&s| s < i16::MIN as i64 || s > i16::MAX as i64)
    {
        return None;
    }
    if !fs.is_finite() || fs <= 0.0 {
        return None;
    }

    let header_bytes = EDF_HEADER_BLOCK + ns * EDF_HEADER_BLOCK;
    let header_str = header_bytes.to_string();
    let max_spr_len = spr.iter().map(|n| n.to_string().len()).max().unwrap_or(1);
    if header_str.len() > 8 || ns.to_string().len() > 4 || max_spr_len > 8 {
        return None;
    }

    let total_samples: usize = spr.iter().sum();
    let mut buf = Vec::with_capacity(header_bytes + total_samples * 2);

    // Main header
    edf_field(&mut buf, "0", 8).ok()?;
    edf_field(&mut buf, "Eagle X X X", 80).ok()?;
    edf_field(&mut buf, "Startdate X", 80).ok()?;
    edf_field(&mut buf, "01.01.26", 8).ok()?;
    edf_field(&mut buf, "00.00.00", 8).ok()?;
    edf_field(&mut buf, &header_str, 8).ok()?;
    edf_field(&mut buf, "", 44).ok()?;
    edf_field(&mut buf, "1", 8).ok()?;
    edf_field(&mut buf, "1       ".trim(), 8).ok()?;
    edf_field(&mut buf, &ns.to_string(), 4).ok()?;

    // Signal headers
    for i in 0..ns {
        edf_field(&mut buf, &format!("ch{i}"), 16).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "AgAgCl", 80).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "uV", 8).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "-32768", 8).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "32767", 8).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "-32768", 8).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "32767", 8).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "", 80).ok()?;
    }
    for &n in &spr {
        edf_field(&mut buf, &n.to_string(), 8).ok()?;
    }
    for _ in 0..ns {
        edf_field(&mut buf, "", 32).ok()?;
    }
    debug_assert_eq!(buf.len(), header_bytes, "EDF header block size mismatch");

    // Data: one record, signals in order, each `spr` little-endian i16.
    for chan in signal {
        for &s in chan {
            buf.extend_from_slice(&(s as i16).to_le_bytes());
        }
    }
    Some(buf)
}

/// Parse per-signal `samples_per_record` from an EDF file and return every
/// channel's samples as little-endian `i64` values.
pub fn read_edf_channels(path: &Path) -> std::io::Result<Vec<Vec<i64>>> {
    let raw = std::fs::read(path)?;
    if raw.len() < EDF_HEADER_BLOCK {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "EDF too short for main header",
        ));
    }
    let ns: usize = std::str::from_utf8(&raw[252..256])
        .unwrap_or("0")
        .trim()
        .parse()
        .unwrap_or(0);
    if ns == 0 {
        return Ok(Vec::new());
    }

    let signal_header_start = EDF_HEADER_BLOCK;
    let header_total = EDF_HEADER_BLOCK + ns * EDF_HEADER_BLOCK;
    if raw.len() < header_total {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "EDF too short for signal headers",
        ));
    }

    // samples_per_record is field 9 in the signal header block.
    let spr_block_start = signal_header_start + ns * 216;
    let mut spr = Vec::with_capacity(ns);
    for i in 0..ns {
        let off = spr_block_start + i * 8;
        let s: usize = std::str::from_utf8(&raw[off..off + 8])
            .unwrap_or("0")
            .trim()
            .parse()
            .unwrap_or(0);
        spr.push(s);
    }

    let mut pos = header_total;
    let mut channels = Vec::with_capacity(ns);
    for &n in &spr {
        let end = pos + n * 2;
        if raw.len() < end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "EDF data record truncated",
            ));
        }
        let samples: Vec<i64> = raw[pos..end]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as i64)
            .collect();
        channels.push(samples);
        pos = end;
    }
    Ok(channels)
}

// ── Scratch directory ──────────────────────────────────────────────

/// A scratch directory under the system temp dir, removed on drop.
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn new(tag: &str) -> std::io::Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let name = format!("eagle_{}_{}_{}_{}", std::process::id(), nanos, seq, tag);
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
