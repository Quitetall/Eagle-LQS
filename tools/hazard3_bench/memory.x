/* Hazard3 testbench memory map — matches Wren6991/Hazard3/test/sim/common/memmap.ld.
 *
 * Hazard3 reset PC = 0x80000040 (config_default.vh) — the first 64
 * bytes hold a vector table, then `j _start` at offset 0x40 hands off
 * to riscv-rt. We carve a small 4 KiB boot region for the
 * trampoline so .text can be laid out normally from 0x80001000.
 */
MEMORY {
    BOOT : ORIGIN = 0x80000000, LENGTH = 4K
    RAM  : ORIGIN = 0x80001000, LENGTH = 4092K
}

REGION_ALIAS("REGION_TEXT",   RAM);
REGION_ALIAS("REGION_RODATA", RAM);
REGION_ALIAS("REGION_DATA",   RAM);
REGION_ALIAS("REGION_BSS",    RAM);
REGION_ALIAS("REGION_HEAP",   RAM);
REGION_ALIAS("REGION_STACK",  RAM);

PROVIDE(_stack_start = ORIGIN(RAM) + LENGTH(RAM));
PROVIDE(_max_hart_id = 0);
PROVIDE(_hart_stack_size = 16K);

/* Boot trampoline — 16 vector slots (4 B each) then `j _start` at
 * offset 0x40. Lives in BOOT so the reset PC of 0x80000040 lands on
 * our jump instruction.
 */
SECTIONS {
    .boot_vectors ORIGIN(BOOT) : ALIGN(4) {
        KEEP(*(.boot_vectors));
        . = ALIGN(0x40);
        KEEP(*(.boot_trampoline));
    } > BOOT
} INSERT BEFORE .text;
