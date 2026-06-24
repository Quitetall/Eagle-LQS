// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Brian Lam
//! `blut-eagle` — Eagle test cookbook binary.
//!
//! Registers the Eagle cookbook with BLUT's CLI.
//! Usage: `blut-eagle recipe run eagle-codec --args '{...}'`

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let reg = blut_cookbook_eagle::registry();
    blut::cli::run(reg).await
}
