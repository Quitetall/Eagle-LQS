//! Host benchmark for the Rust port of the v7.7 firmware kernels.
//!
//! Same iteration count + same xorshift PRNG seed as `bench_kernels_c.c` so
//! a side-by-side comparison is apples-to-apples on the same machine.
//!
//! Build:
//!   cargo build --release --manifest-path tools/bench/bench_rs/Cargo.toml
//! Run:
//!   ./tools/bench/bench_rs/target/release/bench_rs

use std::hint::black_box;
use std::time::Instant;

use lamquant_firmware::dsp::biquad::{HpFilter, HpFilterBank, NUM_CHANNELS, WINDOW_SAMPLES};
use lamquant_firmware::dsp::lifting::{forward_all_channels, LiftingScratch, Subbands};
use lamquant_firmware::dsp::lpc::{analyze_all_channels, LpcOutput};
use lamquant_firmware::neural::ternary_mac;

const N_ITERATIONS: u64 = 100_000_000;
const ARRAY_SIZE: usize = 1024 * 1024;

struct Xs(u64);
impl Xs {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn reset(&mut self) {
        self.0 = 0x4C4D5141_DEADBEEF;
    }
}

fn ns_per(t: u128, n: u64) -> f64 {
    t as f64 / n as f64
}

fn main() {
    let mut prng = Xs(0x4C4D5141_DEADBEEF);

    println!("=== Rust kernels (host x86, rustc release) ===");

    // ── 1. Ternary mac_byte_fast ────────────────────────────────────
    let mut acts: Vec<i16> = Vec::with_capacity(ARRAY_SIZE);
    for _ in 0..ARRAY_SIZE {
        acts.push(prng.next() as i16);
    }
    let mut packs: Vec<u8> = Vec::with_capacity(ARRAY_SIZE / 4);
    for _ in 0..ARRAY_SIZE / 4 {
        let r = prng.next();
        let mut b = 0u8;
        for j in 0..4 {
            let mut w = ((r >> (j * 8)) & 0x03) as u8;
            if w == 0x03 {
                w = 0;
            }
            b |= w << (j * 2);
        }
        packs.push(b);
    }

    // warmup
    let mut warm = 0i32;
    for i in 0..1000usize {
        let pi = i & ((ARRAY_SIZE / 4) - 1);
        let ai = (i * 4) & (ARRAY_SIZE - 4);
        let chunk: &[i16; 4] = acts[ai..ai + 4].try_into().unwrap();
        warm = warm.wrapping_add(ternary_mac::mac_byte_fast(packs[pi], chunk));
    }
    black_box(warm);

    let mut acc: i32 = 0;
    let t0 = Instant::now();
    for i in 0..N_ITERATIONS {
        let pi = (i as usize) & ((ARRAY_SIZE / 4) - 1);
        let ai = ((i as usize) * 4) & (ARRAY_SIZE - 4);
        let chunk: &[i16; 4] = acts[ai..ai + 4].try_into().unwrap();
        acc = acc.wrapping_add(ternary_mac::mac_byte_fast(packs[pi], chunk));
    }
    let t = t0.elapsed().as_nanos();
    black_box(acc);
    let ns = ns_per(t, N_ITERATIONS);
    let g = (N_ITERATIONS as f64 * 4.0) / (t as f64 / 1e9) / 1e9;
    println!(
        "ternary fast     : {:.2} ns/byte    {:.2} Gmac/s   acc={}",
        ns, g, acc
    );

    // ── 1b. conv1d_channel — call-overhead-amortized ────────────────
    const IN_CH: usize = 21;
    const K: usize = 3;
    const TOTAL_W: usize = IN_CH * K;
    const PACKED: usize = (TOTAL_W + 3) / 4;
    let conv_packs = &packs[0..PACKED];
    let conv_acts = &acts[0..64];
    let conv_iters: u64 = (N_ITERATIONS * 4) / TOTAL_W as u64;
    let mut acc: i64 = 0;
    let t0 = Instant::now();
    for _ in 0..conv_iters {
        let r = ternary_mac::conv1d_channel(conv_acts, IN_CH, K, conv_packs, 1 << 30);
        acc = acc.wrapping_add(r as i64);
    }
    let t = t0.elapsed().as_nanos() as f64;
    black_box(acc);
    let g = conv_iters as f64 * TOTAL_W as f64 / (t / 1e9) / 1e9;
    println!(
        "conv1d_channel   : {:.2} ns/call    {:.2} Gmac/s   ({}-MAC kernel)",
        t / conv_iters as f64,
        g,
        TOTAL_W
    );

    // ── 2. Biquad — 21 ch × 2500 samples × 1000 windows ─────────────
    prng.reset();
    let mut signal: Box<[[i32; WINDOW_SAMPLES]; NUM_CHANNELS]> =
        Box::new([[0i32; WINDOW_SAMPLES]; NUM_CHANNELS]);
    for ch in 0..NUM_CHANNELS {
        for i in 0..WINDOW_SAMPLES {
            signal[ch][i] = (prng.next() & 0x3FFF_FFFF) as i32 - 0x1FFF_FFFF;
        }
    }
    let mut bank = HpFilterBank::new();
    let n_windows: u64 = 1000;
    let mut chk: i32 = 0;
    let t0 = Instant::now();
    for _ in 0..n_windows {
        bank.run(
            &mut signal,
            WINDOW_SAMPLES,
            HpFilter::Hz0_5,
            (1 << NUM_CHANNELS) - 1,
        );
        chk ^= signal[0][1234];
    }
    let t = t0.elapsed().as_nanos() as f64;
    black_box(chk);
    let total_samples = (n_windows as f64) * (NUM_CHANNELS as f64) * (WINDOW_SAMPLES as f64);
    println!(
        "biquad 21x2500   : {:.2} us/window  ({:.0} Msamp/s, ns/sample {:.2}, chk={})",
        t / n_windows as f64 / 1000.0,
        total_samples / (t / 1e9) / 1e6,
        t / total_samples,
        chk
    );

    // ── 3. LPC analyze — 21 ch full (read-only on signal) ───────────
    let mut lpc_out = Box::new(LpcOutput::zeroed());
    let n_lpc: u64 = 200;
    let mut chk: i32 = 0;
    let t0 = Instant::now();
    for _ in 0..n_lpc {
        analyze_all_channels(&signal, &mut lpc_out);
        chk ^= lpc_out.residual[0][2400];
    }
    let t = t0.elapsed().as_nanos() as f64;
    black_box(chk);
    println!(
        "LPC analyze 21ch : {:.2} us/window  (in-place i64-internal, no alloc)  chk={}",
        t / n_lpc as f64 / 1000.0,
        chk
    );

    // ── 4. Lifting forward — 21 ch full (mutates signal in place) ───
    // Restore signal from a saved snapshot each iter so we measure pure
    // lift cost, not random-input divergence.
    let snapshot: Box<[[i32; WINDOW_SAMPLES]; NUM_CHANNELS]> = signal.clone();
    let mut subbands = Box::new(Subbands::zeroed());
    let mut scratch = Box::new(LiftingScratch::zeroed());
    let n_lift: u64 = 1000;
    let mut chk: i32 = 0;
    let t0 = Instant::now();
    for _ in 0..n_lift {
        *signal = *snapshot;
        forward_all_channels(&mut signal, &mut scratch, &mut subbands);
        chk ^= subbands.l3_approx[0][100];
    }
    let t = t0.elapsed().as_nanos() as f64;
    black_box(chk);
    println!(
        "lifting 21ch 3lvl: {:.2} us/window  (in-place i32, no alloc)  chk={}",
        t / n_lift as f64 / 1000.0,
        chk
    );
}
