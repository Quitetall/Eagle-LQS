//! LamQuant Quality Suite.
//!
//! LQS is LamQuant's internal EEG codec test suite. It wraps OpenECS
//! (`open-eeg-codec-standard`) for neutral grading primitives and is the home
//! for LamQuant-specific corpus configs, clinical metrics, and batch runners.
pub use open_eeg_codec_standard::adapter;
pub use open_eeg_codec_standard::edf;
pub use open_eeg_codec_standard::harness;
