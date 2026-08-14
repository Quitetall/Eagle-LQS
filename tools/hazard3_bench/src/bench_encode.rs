//! LamQuant lossless encoder cycle bench for Hazard3 (RP2350 RISC-V).
//!
//! Boots in the Hazard3 reference testbench (Wren6991/Hazard3 +
//! tb_verilator OR tb_cxxrtl), synthesises one 21 ch x 2500 sample Q31
//! window of deterministic xorshift noise, runs the lossless encoder
//! pipeline N=8 times, and reports retired-instruction count + cycle
//! count read from the mcycle/minstret CSRs.
//!
//! Reset PC = 0x80000000. All BSS + heap + stack live in the single
//! 1 MiB testbench SRAM. Sim I/O hardware lives at 0xC000_0000 (see
//! external/Hazard3/test/sim/common/tb_cxxrtl_io.h).
//!
//! Build:
//!   cd tools/hazard3_bench && cargo build --release
//!
//! Run (Verilator):
//!   external/Hazard3/test/sim/tb_verilator/tb \
//!       --bin target/riscv32imac-unknown-none-elf/release/bench_encode \
//!       --cycles 200000000

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::{asm, global_asm};

// Boot trampoline. Hazard3 reset PC is 0x80000040 — the first 64 bytes
// at 0x80000000 are a vector table (we fill with `.halt` jumps), then
// at offset 0x40 a single `j _start` hands off to riscv-rt's startup
// (BSS clear, stack init, .data copy, call `main`).
global_asm!(
    r#"
    .section .boot_vectors, "ax"
    .global _boot_vectors_start
_boot_vectors_start:
    .rept 16
        j .L_halt
    .endr

    .section .boot_trampoline, "ax"
    .global _reset_trampoline
_reset_trampoline:
    j _start

    .section .text, "ax"
.L_halt:
    wfi
    j .L_halt
    "#
);

use core::mem::MaybeUninit;
use core::ptr::{addr_of_mut, write_volatile};
use core::sync::atomic::{compiler_fence, Ordering};

use embedded_alloc::LlffHeap as Heap;
use lamquant_firmware::dsp::biquad::{NUM_CHANNELS, WINDOW_SAMPLES};
use lamquant_firmware::pipeline::{
    CodecMode, CompleteWindow, FirmwarePipeline, StageReadiness, TransportFrame, TransportSink,
};
use lamquant_firmware::safety::SafetyState;
use panic_halt as _;

// ─── Hazard3 testbench MMIO (matches common/tb_cxxrtl_io.h) ─────────────────
const IO_BASE: usize = 0xC000_0000;
const IO_PRINT_CHAR: *mut u32 = IO_BASE as *mut u32;
const IO_PRINT_U32: *mut u32 = (IO_BASE + 4) as *mut u32;
const IO_EXIT: *mut u32 = (IO_BASE + 8) as *mut u32;

fn tb_putc(c: u8) {
    unsafe { write_volatile(IO_PRINT_CHAR, c as u32) }
}
fn tb_puts(s: &str) {
    for b in s.as_bytes() {
        tb_putc(*b);
    }
}
fn tb_put_u32(v: u32) {
    unsafe { write_volatile(IO_PRINT_U32, v) }
}
fn tb_exit(code: u32) -> ! {
    unsafe { write_volatile(IO_EXIT, code) }
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

// ─── CSR readers (64-bit composite, RV32-safe) ─────────────────────────────
#[inline(always)]
fn read_mcycle64() -> u64 {
    loop {
        let h0: u32;
        let lo: u32;
        let h1: u32;
        unsafe {
            asm!(
                "csrr {0}, mcycleh",
                "csrr {1}, mcycle",
                "csrr {2}, mcycleh",
                out(reg) h0, out(reg) lo, out(reg) h1,
                options(nomem, nostack, preserves_flags),
            );
        }
        if h0 == h1 {
            return ((h0 as u64) << 32) | (lo as u64);
        }
    }
}

#[inline(always)]
fn read_minstret64() -> u64 {
    loop {
        let h0: u32;
        let lo: u32;
        let h1: u32;
        unsafe {
            asm!(
                "csrr {0}, minstreth",
                "csrr {1}, minstret",
                "csrr {2}, minstreth",
                out(reg) h0, out(reg) lo, out(reg) h1,
                options(nomem, nostack, preserves_flags),
            );
        }
        if h0 == h1 {
            return ((h0 as u64) << 32) | (lo as u64);
        }
    }
}

// ─── Deterministic synthetic input generator (xorshift32) ──────────────────
#[inline(always)]
fn xorshift32(s: &mut u32) -> u32 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *s = x;
    x
}

type Window = [[i32; WINDOW_SAMPLES]; NUM_CHANNELS];

/// Fill pipeline-owned acquisition storage outside measured encode brackets.
unsafe fn fill_window(ptr: *mut Window, seed0: u32) {
    let signal = &mut *ptr;
    let mut seed = seed0;
    for channel in signal {
        for sample in channel {
            let value = xorshift32(&mut seed) as i32;
            *sample = (value >> 11) & 0x000F_FFFF;
        }
    }
}

struct DiscardSink;

impl TransportSink for DiscardSink {
    type Error = core::convert::Infallible;

    fn send(&mut self, _frame: TransportFrame<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
}

// ─── Global allocator (codec uses transient Vec in entropy stage) ──────────
#[global_allocator]
static HEAP: Heap = Heap::empty();
const HEAP_SIZE: usize = 96 * 1024;

#[link_section = ".bss"]
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

// ─── Pipeline + signal state (BSS-resident, sized once at link time) ───────
#[link_section = ".bss"]
static mut PIPELINE: MaybeUninit<FirmwarePipeline> = MaybeUninit::uninit();
#[link_section = ".bss"]
static mut SAFETY: MaybeUninit<SafetyState> = MaybeUninit::uninit();

const ITERS: u32 = 8;
const CORE_CLOCK_MHZ: u64 = 150;

#[riscv_rt::entry]
fn main() -> ! {
    // Hazard3 mcycle/minstret counters are inhibited at reset:
    // mcountinhibit has bits 0+2 set (`CY` gates mcycle, `IR` gates
    // minstret). `csrci mcountinhibit, 0x5` clears those bits and
    // enables the counters. Must run before any timing read. M-mode
    // is implicit under riscv-rt's _start.
    unsafe {
        asm!("csrci mcountinhibit, 0x5", options(nomem, nostack));
    }

    let pipeline_ptr = addr_of_mut!(PIPELINE).cast::<FirmwarePipeline>();
    let safety_ptr = addr_of_mut!(SAFETY).cast::<SafetyState>();
    unsafe {
        HEAP.init(addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE);
        FirmwarePipeline::init_in_place(pipeline_ptr);
        SafetyState::init_in_place(safety_ptr);
        (*safety_ptr).init(0);
    }

    let pipeline: &mut FirmwarePipeline = unsafe { &mut *pipeline_ptr };
    let safety: &mut SafetyState = unsafe { &mut *safety_ptr };
    pipeline.set_codec_mode(CodecMode::Lossless);
    pipeline.boot(StageReadiness::all_ready()).unwrap();
    let signal: *mut Window = unsafe { addr_of_mut!((*pipeline_ptr).lpc.residual) };
    let mut sink = DiscardSink;

    tb_puts("=== LamQuant lossless encoder cycle bench ===\n");
    tb_puts("target=Hazard3 RV32IMACZba_Zbb_Zbkb_Zbs (RP2350 silicon config)\n");
    tb_puts("window=21ch x 2500samp @ 250Hz (10 s per window, 52500 samples/window)\n");
    tb_puts("iters=");
    tb_put_u32(ITERS);
    tb_puts("\n");

    // Warm-up — cache lines / allocator. Input fill remains outside measured
    // region so reported cycles cover typed pipeline processing only.
    unsafe { fill_window(signal, 0xCAFE_BABE) };
    let warmup = pipeline
        .process_window(CompleteWindow::new(0), safety, &mut sink, 0)
        .unwrap();
    core::hint::black_box(&warmup);

    let mut cycles_total = 0u64;
    let mut instrs_total = 0u64;
    for i in 0..ITERS {
        unsafe { fill_window(signal, 0xCAFE_BABE ^ i.wrapping_mul(0x9E37_79B9)) };
        compiler_fence(Ordering::SeqCst);
        let c0 = read_mcycle64();
        let i0 = read_minstret64();
        compiler_fence(Ordering::SeqCst);
        let r = pipeline
            .process_window(CompleteWindow::new(i + 1), safety, &mut sink, 0)
            .unwrap();
        compiler_fence(Ordering::SeqCst);
        let i1 = read_minstret64();
        let c1 = read_mcycle64();
        cycles_total += c1 - c0;
        instrs_total += i1 - i0;
        core::hint::black_box(&r);
    }
    let cycles_per_window = (cycles_total / ITERS as u64) as u32;
    let instrs_per_window = (instrs_total / ITERS as u64) as u32;
    let cpi_x1000 = ((cycles_total.saturating_mul(1000)) / instrs_total.max(1)) as u32;
    // wall-clock at 150 MHz, microseconds
    let window_us = (cycles_per_window as u64 / CORE_CLOCK_MHZ) as u32;
    let samples_per_window: u64 = (NUM_CHANNELS as u64) * (WINDOW_SAMPLES as u64);
    // Msa/s × 100 (fixed-point, divide by 100 host-side for the real number)
    let msa_per_s_x100 =
        ((samples_per_window.saturating_mul(100)) / window_us.max(1) as u64) as u32;

    tb_puts("cycles_per_window=");
    tb_put_u32(cycles_per_window);
    tb_puts("instrs_per_window=");
    tb_put_u32(instrs_per_window);
    tb_puts("CPI_x1000=");
    tb_put_u32(cpi_x1000);
    tb_puts("window_us@150MHz=");
    tb_put_u32(window_us);
    tb_puts("Msa_per_s_x100=");
    tb_put_u32(msa_per_s_x100);
    tb_puts("=== END BENCH ===\n");

    tb_exit(0);
}
