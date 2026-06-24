//! Eagle — LamQuant's validation suite.
//!
//! Eagle wraps OpenECS (`open-eeg-codec-standard`) for vendor-neutral
//! grading primitives and adds the LamQuant-specific pieces:
//!
//!   - [`adapters_lamquant`] — a [`adapter::Codec`] adapter that shells
//!     to the real `lml` lossless codec binary.
//!   - LQS compliance, benchmarks, clinical validation.
//!
//! Re-exports from OpenECS for convenience:
//!   - [`adapter`] — codec adapter trait + reference codecs.
//!   - [`edf`] — EDF file reader.
//!   - [`harness`] — grading harness (PRD/R/SNR/CR + tiers L/C/M/A).

pub use open_eeg_codec_standard::adapter;
pub use open_eeg_codec_standard::edf;
pub use open_eeg_codec_standard::harness;

pub mod adapters_lamquant;

pub use adapters_lamquant::LamQuantLossless;
