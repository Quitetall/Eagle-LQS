//! `eagle-lqs` — CLI front-end for the LQS vendor-neutral EEG codec
//! benchmark standard (LQS v1.0; see `SPEC/LQS-v1.0.md`).
//!
//! ## Legacy single-signal form (unchanged)
//!
//! ```text
//! eagle-lqs [CODEC] [FILE.edf]
//! ```
//!
//! `CODEC` is `store` (default), `gzip`, or `quantize` (a lossy demo). With
//! a second argument it grades that EDF; otherwise a built-in synthetic
//! fixture. Exit: 0 pass, 1 below-floor, 2 unknown codec, 3 EDF read error.
//!
//! ## LQS v1.0 subcommands
//!
//! ```text
//! eagle-lqs grade [--codec NAME | --codec-manifest C.toml]
//!                 [--corpus-manifest K.toml | --edf FILE]
//!                 [--out submission.json] [--leaderboard] [--dump-recon DIR]
//! eagle-lqs verify-corpus --corpus-manifest K.toml
//! eagle-lqs emit-corpus-manifest --root DIR --name NAME --version VER
//! ```
//!
//! `grade` runs a built-in or manifest-defined external codec over a
//! hash-pinned corpus (or a single EDF / the synthetic fixture) and emits a
//! results submission. `verify-corpus` checks a corpus's SHA-256 pins.
//! `emit-corpus-manifest` walks a directory of `.edf` files and prints a
//! manifest (with hashes + shapes) to stdout. Extra exit codes: 4 corpus
//! integrity failure, 5 manifest load/parse error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lqs::adapter::{deserialize, serialize, Codec, Gzip, Store};
use lqs::corpus::{self, sha256_hex};
use lqs::manifest;
use lqs::report::{leaderboard, CodecIdentity, CorpusIdentity, LqsReport, LqsSubmission};
use lqs::subprocess::write_edf_bytes;
use lqs::{edf, harness};

/// A deliberately-lossy demo codec: quantize by integer division by `STEP`
/// on encode, multiply back on decode. Not bit-exact — exercises the lossy
/// battery. Lives in the CLI as a demonstration.
struct Quantize {
    step: i64,
}

impl Codec for Quantize {
    fn name(&self) -> &str {
        "quantize"
    }
    fn declared_lossless(&self) -> bool {
        false
    }
    fn encode(&self, signal: &[Vec<i64>], _fs: f64) -> Vec<u8> {
        let q: Vec<Vec<i64>> = signal
            .iter()
            .map(|chan| chan.iter().map(|&s| s / self.step).collect())
            .collect();
        serialize(&q)
    }
    fn decode(&self, blob: &[u8]) -> Vec<Vec<i64>> {
        deserialize(blob)
            .into_iter()
            .map(|chan| chan.into_iter().map(|s| s * self.step).collect())
            .collect()
    }
}

/// Build a synthetic multichannel EEG-like signal (deterministic).
fn synthetic_signal(n_chan: usize, n: usize, fs: f64) -> Vec<Vec<i64>> {
    use std::f64::consts::PI;
    (0..n_chan)
        .map(|c| {
            let amp = 1.0 + 0.3 * c as f64;
            (0..n)
                .map(|i| {
                    let t = i as f64 / fs;
                    let v = amp
                        * (40.0
                            + 120.0 * (2.0 * PI * 2.0 * t).sin()
                            + 80.0 * (2.0 * PI * 6.0 * t).sin()
                            + 60.0 * (2.0 * PI * 10.0 * t).sin()
                            + 30.0 * (2.0 * PI * 20.0 * t).sin()
                            + 15.0 * (2.0 * PI * 40.0 * t).sin());
                    v.round() as i64
                })
                .collect()
        })
        .collect()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("grade") => cmd_grade(&args[1..]),
        Some("verify-corpus") => cmd_verify_corpus(&args[1..]),
        Some("emit-corpus-manifest") => cmd_emit_manifest(&args[1..]),
        _ => cmd_legacy(&args),
    }
}

// ─── flag parsing (std-only, no clap) ──────────────────────────────────────

/// Value following `--name`, if present.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Whether a bare `--name` switch is present.
fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

// ─── legacy single-signal form ─────────────────────────────────────────────

fn cmd_legacy(args: &[String]) -> ExitCode {
    let codec_name = args.first().cloned().unwrap_or_else(|| "store".to_string());
    let file_arg = args.get(1);

    println!("LQS — vendor-neutral EEG codec benchmark standard (v{})", lqs::SPEC_VERSION);

    let (signal, fs) = match file_arg {
        Some(path) => match edf::read_edf(path) {
            Ok(e) => {
                println!(
                    "Source: {} ({} channels @ {} Hz, {} samples/ch) [EDF]\n",
                    path,
                    e.channels.len(),
                    e.fs,
                    e.channels.first().map(|c| c.len()).unwrap_or(0),
                );
                (e.channels, e.fs)
            }
            Err(err) => {
                eprintln!("error: failed to read EDF file '{path}': {err}");
                return ExitCode::from(3);
            }
        },
        None => {
            let fs = 256.0;
            let signal = synthetic_signal(4, 512, fs);
            println!(
                "Fixture: {} channels x {} samples @ {} Hz (synthetic)\n",
                signal.len(),
                signal.first().map(|c| c.len()).unwrap_or(0),
                fs,
            );
            (signal, fs)
        }
    };

    let codec: Box<dyn Codec> = match codec_name.as_str() {
        "store" => Box::new(Store),
        "gzip" => Box::new(Gzip),
        "quantize" => Box::new(Quantize { step: 8 }),
        other => {
            eprintln!("unknown codec '{other}'; valid: store | gzip | quantize");
            return ExitCode::from(2);
        }
    };

    let report = harness::run(codec.as_ref(), &signal, fs);
    print!("{}", report.human_table());
    println!("\n{}", report_badge(&report));

    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// One-line LQS badge from a report's grade.
fn report_badge(report: &LqsReport) -> String {
    if report.passed() {
        format!("LQS-{} COMPLIANT", report.grade)
    } else {
        "LQS NON-COMPLIANT (below alerting floor)".to_string()
    }
}

// ─── grade ─────────────────────────────────────────────────────────────────

/// Resolve the codec under test + its submission identity.
///
/// `--codec-manifest` (external, any language) takes precedence over
/// `--codec` (built-in `store`/`gzip`/`quantize`, default `store`).
fn resolve_codec(args: &[String]) -> Result<(Box<dyn Codec>, CodecIdentity), ExitCode> {
    if let Some(path) = flag(args, "--codec-manifest") {
        let manifest = match manifest::load_codec_manifest(path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {e}");
                return Err(ExitCode::from(5));
            }
        };
        let codec = match manifest.into_adapter() {
            Some(c) => c,
            None => {
                eprintln!(
                    "error: codec '{}' command could not be resolved on this host \
                     (set $LQS_CODEC_<NAME>_BIN or fix the manifest `cmd`)",
                    manifest.codec.name
                );
                return Err(ExitCode::from(5));
            }
        };
        let sha = std::fs::read(path).map(|b| sha256_hex(&b)).ok();
        let id = CodecIdentity {
            name: codec.name().to_string(),
            manifest_sha256: sha,
        };
        return Ok((Box::new(codec), id));
    }

    let name = flag(args, "--codec").unwrap_or("store");
    let codec: Box<dyn Codec> = match name {
        "store" => Box::new(Store),
        "gzip" => Box::new(Gzip),
        "quantize" => Box::new(Quantize { step: 8 }),
        other => {
            eprintln!("unknown codec '{other}'; valid: store | gzip | quantize, or --codec-manifest");
            return Err(ExitCode::from(2));
        }
    };
    let id = CodecIdentity {
        name: name.to_string(),
        manifest_sha256: None,
    };
    Ok((codec, id))
}

fn cmd_grade(args: &[String]) -> ExitCode {
    let (codec, codec_id) = match resolve_codec(args) {
        Ok(v) => v,
        Err(code) => return code,
    };

    // Signal source: a hash-pinned corpus, a single EDF, or the synthetic
    // fixture (so `grade` runs anywhere).
    if let Some(corpus_path) = flag(args, "--corpus-manifest") {
        return grade_corpus(codec.as_ref(), &codec_id, corpus_path, args);
    }

    let (signal, fs, dataset) = match flag(args, "--edf") {
        Some(path) => match edf::read_edf(path) {
            Ok(e) => (e.channels, e.fs, path.to_string()),
            Err(err) => {
                eprintln!("error: failed to read EDF '{path}': {err}");
                return ExitCode::from(3);
            }
        },
        None => (synthetic_signal(4, 512, 256.0), 256.0, "(synthetic)".to_string()),
    };

    let mut report = harness::run(codec.as_ref(), &signal, fs);
    report.dataset = dataset.clone();
    print!("{}", report.human_table());
    println!("\n{}", report_badge(&report));

    // A single-file submission (corpus of one) for --out parity.
    if let Some(out) = flag(args, "--out") {
        let (reports, summary) = harness::run_corpus(codec.as_ref(), &[(signal, fs)]);
        let mut reports = reports;
        for r in &mut reports {
            r.dataset = dataset.clone();
        }
        let submission = LqsSubmission::new(
            codec_id,
            CorpusIdentity { name: dataset, version: "n/a".to_string() },
            reports,
            summary,
        );
        if let Err(e) = std::fs::write(out, submission.to_json()) {
            eprintln!("error: writing submission '{out}': {e}");
            return ExitCode::FAILURE;
        }
        println!("\nwrote submission: {out}");
    }

    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn grade_corpus(
    codec: &dyn Codec,
    codec_id: &CodecIdentity,
    corpus_path: &str,
    args: &[String],
) -> ExitCode {
    let manifest = match corpus::load_corpus_manifest(corpus_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(5);
        }
    };
    let base = Path::new(corpus_path).parent().unwrap_or_else(|| Path::new("."));
    let files = match corpus::verify_and_load(&manifest, base) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: corpus integrity/shape: {e}");
            return ExitCode::from(4);
        }
    };

    let (mut reports, summary) = harness::run_corpus(codec, &files);
    for r in &mut reports {
        r.dataset = manifest.name.clone();
    }

    println!(
        "LQS grade — {} on {} v{} ({} files)\n",
        codec_id.name,
        manifest.name,
        manifest.version,
        files.len()
    );
    if has_flag(args, "--leaderboard") {
        print!("{}", leaderboard(&reports));
        println!();
    }
    let worst = if summary.worst_grade == '\0' {
        "— (below floor)".to_string()
    } else {
        format!("LQS-{}", summary.worst_grade)
    };
    println!(
        "  worst grade : {worst}\n  pooled CR   : {:.2} : 1\n  mean R      : {:.4}\n  mean PRD    : {:.3} %\n  bit-exact   : {}",
        summary.mean_cr, summary.mean_r, summary.mean_prd, summary.all_bit_exact
    );

    // Optionally dump reconstructions for the task-concordance tool.
    if let Some(dir) = flag(args, "--dump-recon") {
        if let Err(e) = dump_recon(codec, &files, &manifest.name, dir) {
            eprintln!("warning: --dump-recon: {e}");
        } else {
            println!("\ndumped originals + reconstructions to: {dir}");
        }
    }

    let submission = LqsSubmission::new(
        codec_id.clone(),
        CorpusIdentity { name: manifest.name.clone(), version: manifest.version.clone() },
        reports,
        summary.clone(),
    );
    if let Some(out) = flag(args, "--out") {
        if let Err(e) = std::fs::write(out, submission.to_json()) {
            eprintln!("error: writing submission '{out}': {e}");
            return ExitCode::FAILURE;
        }
        println!("\nwrote submission: {out}");
    }

    if summary.worst_grade != '\0' {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Write paired `orig_{i}.edf` / `recon_{i}.edf` for each corpus file, so
/// the codec-agnostic task-concordance tool (`tools/lqs_task_concordance.py`)
/// can compare originals to reconstructions. Files whose recon cannot be
/// expressed as EDF (e.g. a lossy codec pushing samples out of i16 range)
/// are skipped with a note.
fn dump_recon(
    codec: &dyn Codec,
    files: &[(Vec<Vec<i64>>, f64)],
    dataset: &str,
    dir: &str,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (i, (signal, fs)) in files.iter().enumerate() {
        let blob = codec.encode(signal, *fs);
        let recon = codec.decode(&blob);
        let orig_edf = write_edf_bytes(signal, *fs);
        let recon_edf = write_edf_bytes(&recon, *fs);
        match (orig_edf, recon_edf) {
            (Some(o), Some(r)) => {
                std::fs::write(PathBuf::from(dir).join(format!("orig_{i}.edf")), o)?;
                std::fs::write(PathBuf::from(dir).join(format!("recon_{i}.edf")), r)?;
            }
            _ => eprintln!(
                "  skip recon dump for {dataset} file {i}: not EDF-expressible (ragged/out-of-range)"
            ),
        }
    }
    Ok(())
}

// ─── verify-corpus ─────────────────────────────────────────────────────────

fn cmd_verify_corpus(args: &[String]) -> ExitCode {
    let path = match flag(args, "--corpus-manifest") {
        Some(p) => p,
        None => {
            eprintln!("usage: eagle-lqs verify-corpus --corpus-manifest K.toml");
            return ExitCode::from(5);
        }
    };
    let manifest = match corpus::load_corpus_manifest(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(5);
        }
    };
    let base = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
    println!("verifying corpus {} v{} ({} files)", manifest.name, manifest.version, manifest.file.len());
    match corpus::verify_and_load(&manifest, base) {
        Ok(files) => {
            for (entry, (sig, _)) in manifest.file.iter().zip(&files) {
                println!("  PASS  {} ({} ch x {} samp)", entry.path, sig.len(), sig.first().map(|c| c.len()).unwrap_or(0));
            }
            println!("\nOK: all {} files verified (sha256 + shape).", files.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("  FAIL  {e}");
            ExitCode::from(4)
        }
    }
}

// ─── emit-corpus-manifest ──────────────────────────────────────────────────

fn cmd_emit_manifest(args: &[String]) -> ExitCode {
    let root = match flag(args, "--root") {
        Some(r) => r,
        None => {
            eprintln!("usage: eagle-lqs emit-corpus-manifest --root DIR --name NAME --version VER");
            return ExitCode::from(5);
        }
    };
    let name = flag(args, "--name").unwrap_or("corpus");
    let version = flag(args, "--version").unwrap_or("1.0.0");

    let mut edfs = Vec::new();
    if let Err(e) = collect_edfs(Path::new(root), &mut edfs) {
        eprintln!("error: walking '{root}': {e}");
        return ExitCode::from(3);
    }
    edfs.sort();

    println!("spec_version = \"{}\"", lqs::SPEC_VERSION);
    println!("name = \"{name}\"");
    println!("version = \"{version}\"");
    println!();

    let root_path = Path::new(root);
    let mut emitted = 0usize;
    for path in &edfs {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("warning: skip {} ({e})", path.display());
                continue;
            }
        };
        let sig = match edf::read_edf(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: skip {} (not readable EDF: {e})", path.display());
                continue;
            }
        };
        let n_chan = sig.channels.len();
        let n_samples = sig.channels.first().map(|c| c.len()).unwrap_or(0);
        if n_chan == 0 || !sig.channels.iter().all(|c| c.len() == n_samples) {
            eprintln!("warning: skip {} (empty or ragged channels)", path.display());
            continue;
        }
        let rel = path.strip_prefix(root_path).unwrap_or(path);
        println!("[[file]]");
        println!("path = \"{}\"", rel.display());
        println!("sha256 = \"{}\"", sha256_hex(&bytes));
        // `{:?}` renders a whole number with a decimal (256.0, not 256) so
        // the TOML `fs` field is unambiguously a float.
        println!("fs = {:?}", sig.fs);
        println!("n_chan = {n_chan}");
        println!("n_samples = {n_samples}");
        println!();
        emitted += 1;
    }
    eprintln!("emitted {emitted} file entries from {}", root);
    if emitted == 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Recursively collect `*.edf` files under `dir` into `out`.
fn collect_edfs(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_edfs(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("edf")).unwrap_or(false) {
            out.push(path);
        }
    }
    Ok(())
}
