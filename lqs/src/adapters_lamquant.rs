//! Real LamQuant-Lossless codec adapter.
//!
//! Where [`crate::adapter`] ships the always-available reference codecs
//! (`store`, `gzip`, optional `zstd`), this module wires the **actual
//! production lossless codec** — the `lml` CLI from the sibling
//! LamQuant-Lossless workspace — behind the same [`Codec`] interface so
//! LQS grades the real `.lml` wire format, not a stand-in.
//!
//! ## Why a subprocess and not a crate dependency
//!
//! `lml` operates on **files** (EDF in, raw int32 LE out), not in-memory
//! arrays, and lives in a separate workspace. Linking it as a library
//! would couple the (vendor-neutral) LQS crate to the LamQuant-Lossless
//! build. Instead the adapter shells out to a prebuilt `lml` binary. The
//! default CI build does **not** require that binary to be present:
//!
//! - [`LamQuantLossless::resolve`] returns `None` when no `lml` can be
//!   found, so callers can skip the adapter cleanly.
//! - The `#[cfg(test)]` round-trip below early-returns (printing a skip
//!   note) when `lml` is absent, so `cargo test -p lqs` is green with or
//!   without the sibling workspace checked out.
//!
//! No Cargo feature gate is needed: the module is pure `std` (process
//! spawn + temp files), so it always compiles; availability is a
//! *runtime* property of the host, resolved by [`resolve_lml_bin`].
//!
//! ## Binary resolution order
//!
//! 1. `$LML_BIN` if set (and the path exists).
//! 2. The prebuilt sibling default
//!    `/tmp/lamquant-verify/LamQuant-Lossless/target/debug/lml`.
//! 3. `lml` on `PATH`.
//!
//! ## Round-trip contract
//!
//! `lml` only handles the EDF **digital** sample domain, which is signed
//! 16-bit, and the bundled EDF reader requires a single shared sample
//! rate (hence equal-length channels). The adapter therefore round-trips
//! signals that satisfy:
//!
//! - every sample in `[i16::MIN, i16::MAX]`, and
//! - every channel the same length, and
//! - at least one channel with at least one sample.
//!
//! A signal that violates these cannot be expressed as a `.lml` and
//! [`encode`] returns an empty blob; [`decode`] of an empty blob returns
//! an empty signal, which the harness's L-tier gate then reports as a
//! length/value mismatch (a failed lossless claim) rather than a panic.
//! EEG digital ADC samples are 16-bit by construction, so this covers the
//! real corpus; the synthetic and reference fixtures are sized to fit.
//!
//! [`encode`]: Codec::encode
//! [`decode`]: Codec::decode

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::adapter::Codec;
use crate::subprocess::{reshape_channel_major, write_edf_bytes, SampleDtype, ScratchDir};

/// Default location of the prebuilt sibling `lml` binary.
const DEFAULT_LML_BIN: &str = "/tmp/lamquant-verify/LamQuant-Lossless/target/debug/lml";

/// Resolve the `lml` binary to invoke, or `None` if none is usable.
///
/// Tries `$LML_BIN`, then the prebuilt sibling default, then `lml` on
/// `PATH` (probed by running `lml --version`). Returns the resolved path
/// only when a candidate actually exists / is runnable, so callers can
/// treat `None` as "skip the real-codec adapter on this host".
pub fn resolve_lml_bin() -> Option<PathBuf> {
    // 1. Explicit override via env var.
    if let Some(p) = std::env::var_os("LML_BIN") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
        // An LML_BIN that does not point at a file is a misconfiguration;
        // fall through to the other candidates rather than fail hard.
    }

    // 2. Prebuilt sibling default.
    let default = Path::new(DEFAULT_LML_BIN);
    if default.is_file() {
        return Some(default.to_path_buf());
    }

    // 3. `lml` on PATH — probe by actually running it, since we cannot
    //    portably stat PATH entries. `--version` is cheap and side-effect
    //    free; a non-zero/failed spawn means no usable `lml`.
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

/// The real LamQuant-Lossless codec, driven through the `lml` CLI.
///
/// Declared lossless: every supported signal (see the module docs) round
/// trips bit-exactly through the production `.lml` wire format. Construct
/// via [`LamQuantLossless::resolve`] (auto-discovers `lml`) or
/// [`LamQuantLossless::with_bin`] (explicit path).
#[derive(Clone, Debug)]
pub struct LamQuantLossless {
    /// Path to the `lml` binary this adapter shells out to.
    lml_bin: PathBuf,
    /// Sample rate carried into the temp EDF header (metadata only — the
    /// integer-domain samples round-trip independent of it).
    fs: f64,
}

impl LamQuantLossless {
    /// Construct the adapter if a usable `lml` binary can be found.
    ///
    /// Returns `None` when no `lml` is available (see [`resolve_lml_bin`]),
    /// so the default CI build / a host without the sibling workspace can
    /// skip the real-codec adapter without error.
    pub fn resolve(fs: f64) -> Option<Self> {
        resolve_lml_bin().map(|lml_bin| Self { lml_bin, fs })
    }

    /// Construct the adapter against an explicit `lml` binary path.
    pub fn with_bin(lml_bin: PathBuf, fs: f64) -> Self {
        Self { lml_bin, fs }
    }

    /// The resolved `lml` binary path this adapter uses.
    pub fn lml_bin(&self) -> &Path {
        &self.lml_bin
    }
}

/// Parse the per-channel sample count and channel count from `lml info`.
///
/// `lml info` prints `Channels:   N` and `Samples:    M (...)` lines. We
/// scan for both; the flat int32 decode stream is `N * M` samples in
/// channel-major order, so these two numbers reconstruct the shape
/// without trusting the stream length alone.
fn parse_info_shape(info_stdout: &str) -> Option<(usize, usize)> {
    let mut channels: Option<usize> = None;
    let mut samples: Option<usize> = None;
    for line in info_stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Channels:") {
            channels = rest.split_whitespace().next()?.parse().ok();
        } else if let Some(rest) = line.strip_prefix("Samples:") {
            // "Samples:    8 (2.0s @ 4 Hz)" -> first token is the count.
            samples = rest.split_whitespace().next()?.parse().ok();
        }
    }
    match (channels, samples) {
        (Some(c), Some(s)) if c > 0 => Some((c, s)),
        _ => None,
    }
}

impl LamQuantLossless {
    /// Encode `signal` to production `.lml` bytes, or `Vec::new()` on any
    /// failure (unsupported signal shape, `lml` error, I/O error).
    fn try_encode(&self, signal: &[Vec<i64>], fs: f64) -> Vec<u8> {
        let edf = match write_edf_bytes(signal, fs) {
            Some(e) => e,
            None => return Vec::new(),
        };
        let dir = match ScratchDir::new("enc") {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };
        let edf_path = dir.join("in.edf");
        if std::fs::write(&edf_path, &edf).is_err() {
            return Vec::new();
        }

        // `--no-bundle --i-understand-data-loss` emits a bare `.lml` (the
        // real wire format) rather than the per-recording `.lma` envelope.
        // `-q` silences progress so only errors reach stderr.
        let status = Command::new(&self.lml_bin)
            .arg("encode")
            .arg(&edf_path)
            .arg("-o")
            .arg(&dir.path)
            .arg("--no-bundle")
            .arg("--i-understand-data-loss")
            .arg("-q")
            .output();
        match status {
            Ok(o) if o.status.success() => {}
            _ => return Vec::new(),
        }

        // `lml` names the output after the input stem: in.edf -> in.lml.
        let lml_path = dir.join("in.lml");
        std::fs::read(&lml_path).unwrap_or_default()
    }

    /// Decode production `.lml` bytes back to the per-channel signal, or
    /// `Vec::new()` on any failure (so the L-tier gate sees a mismatch).
    fn try_decode(&self, blob: &[u8]) -> Vec<Vec<i64>> {
        if blob.is_empty() {
            return Vec::new();
        }
        let dir = match ScratchDir::new("dec") {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };
        let lml_path = dir.join("in.lml");
        if std::fs::write(&lml_path, blob).is_err() {
            return Vec::new();
        }

        // Read the channel/sample shape from the container metadata so the
        // flat int32 stream can be split into channel-major chunks.
        let info = Command::new(&self.lml_bin)
            .arg("info")
            .arg(&lml_path)
            .arg("-q")
            .output();
        let (n_chan, per_chan) = match info {
            Ok(o) if o.status.success() => {
                match parse_info_shape(&String::from_utf8_lossy(&o.stdout)) {
                    Some(shape) => shape,
                    None => return Vec::new(),
                }
            }
            _ => return Vec::new(),
        };

        let raw_path = dir.join("out.raw");
        let dec = Command::new(&self.lml_bin)
            .arg("decode")
            .arg(&lml_path)
            .arg("-o")
            .arg(&raw_path)
            .arg("-q")
            .output();
        match dec {
            Ok(o) if o.status.success() => {}
            _ => return Vec::new(),
        }

        let raw = match std::fs::read(&raw_path) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        // `lml` decode emits a little-endian int32, channel-major stream;
        // the shared reshape validates the byte length against the declared
        // shape before splitting (an empty Vec on mismatch => failed claim).
        reshape_channel_major(&raw, n_chan, per_chan, SampleDtype::I32).unwrap_or_default()
    }
}

impl Codec for LamQuantLossless {
    fn name(&self) -> &str {
        "lamquant-lossless"
    }

    fn declared_lossless(&self) -> bool {
        true
    }

    fn encode(&self, signal: &[Vec<i64>], fs: f64) -> Vec<u8> {
        // Prefer the rate the harness passes; fall back to the adapter's
        // configured fs only if the caller hands us a non-positive rate.
        let rate = if fs.is_finite() && fs > 0.0 { fs } else { self.fs };
        self.try_encode(signal, rate)
    }

    fn decode(&self, blob: &[u8]) -> Vec<Vec<i64>> {
        self.try_decode(blob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness;

    /// A known multi-channel i64 signal that fits the EDF digital domain
    /// (every value in the i16 range) and has equal-length channels, so it
    /// is expressible as a `.lml`.
    ///
    /// It is deliberately corpus-sized (a few thousand samples per
    /// channel) and EEG-like (smooth, band-limited, integer ADC counts):
    /// the production codec needs a real window of correlated data to
    /// compress below the raw 8-bytes/sample reference container and clear
    /// the harness's L-tier `cr >= 0.8` floor. A handful of samples is
    /// dominated by container overhead and would (correctly) grade below
    /// the floor, so the fixture mirrors the shape of real data the codec
    /// is built for. Deterministic — no RNG — so the round trip is
    /// reproducible.
    fn fixture() -> Vec<Vec<i64>> {
        use std::f64::consts::PI;
        let fs = 256.0;
        let n = 4096;
        (0..4)
            .map(|c| {
                let amp = 1.0 + 0.25 * c as f64;
                (0..n)
                    .map(|i| {
                        let t = i as f64 / fs;
                        // EEG-band sinusoids scaled to a few hundred ADC
                        // counts — comfortably inside the i16 digital range.
                        let v = amp
                            * (80.0 * (2.0 * PI * 2.0 * t).sin()
                                + 50.0 * (2.0 * PI * 10.0 * t).sin()
                                + 25.0 * (2.0 * PI * 22.0 * t).sin()
                                + 10.0 * (2.0 * PI * 40.0 * t).sin());
                        v.round() as i64
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn fixture_is_expressible_as_edf() {
        // The EDF writer + reshape primitives now live in
        // `crate::subprocess` (tested there). Here we only confirm the
        // corpus-sized fixture this adapter feeds `lml` is EDF-expressible.
        let sig = fixture();
        assert!(write_edf_bytes(&sig, 256.0).is_some(), "fixture -> EDF");
    }

    #[test]
    fn parse_info_shape_reads_channels_and_samples() {
        let info = "File:       x.lml\nChannels:   3\nWindows:    1\nSamples:    8 (2.0s @ 4 Hz)\n";
        assert_eq!(parse_info_shape(info), Some((3, 8)));
        // Missing fields -> None, never a panic.
        assert_eq!(parse_info_shape("nothing useful here"), None);
        assert_eq!(parse_info_shape("Channels:   0\nSamples:    4"), None);
    }

    #[test]
    fn empty_blob_decodes_to_empty_signal() {
        // Decode side must be panic-free on an empty/absent blob even
        // without lml; an empty blob short-circuits before any spawn.
        let codec = LamQuantLossless::with_bin(PathBuf::from("/nonexistent/lml"), 256.0);
        assert!(codec.decode(&[]).is_empty());
        // declared_lossless / name are pure and need no binary.
        assert!(codec.declared_lossless());
        assert_eq!(codec.name(), "lamquant-lossless");
    }

    /// End-to-end round trip through the REAL `lml` binary.
    ///
    /// Gated on `lml` availability: if no binary resolves, the test prints
    /// a skip note and passes, so `cargo test -p lqs` is green on a host
    /// without the sibling LamQuant-Lossless workspace.
    #[test]
    fn lml_roundtrip_grades_lqs_l_when_available() {
        let codec = match LamQuantLossless::resolve(256.0) {
            Some(c) => c,
            None => {
                eprintln!(
                    "SKIP lml_roundtrip_grades_lqs_l_when_available: \
                     no lml binary found (set LML_BIN, build the sibling \
                     LamQuant-Lossless workspace, or put `lml` on PATH)"
                );
                return;
            }
        };
        eprintln!("using lml binary: {}", codec.lml_bin().display());

        let signal = fixture();

        // (a) Direct adapter round trip is bit-exact.
        let blob = codec.encode(&signal, 256.0);
        assert!(
            !blob.is_empty(),
            "lml encode produced no .lml bytes for the fixture"
        );
        let back = codec.decode(&blob);
        assert_eq!(
            back, signal,
            "lamquant-lossless failed bit-exact round trip through real lml"
        );

        // (b) The harness grades it LQS-L (bit-exact integer domain).
        let report = harness::run(&codec, &signal, 256.0);
        assert!(report.bit_exact, "real-codec round trip must be bit-exact");
        assert_eq!(report.grade, 'L', "bit-exact codec must grade LQS-L");
        assert_eq!(report.prd, 0.0);
        assert_eq!(report.r, 1.0);
    }
}
