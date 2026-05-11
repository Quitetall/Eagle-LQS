/*
 * bench_kernels_c.c — host benchmark for v7.7 firmware C kernels.
 *
 * Inlines the prod kernels (or their hot inner loops) so we can compare
 * apples-to-apples against the Rust port without dragging in HAL deps.
 *
 * Kernels:
 *   1. ternary mac_byte_fast (production branchless)
 *   2. ternary mac_byte_lut  (reference / x86-friendly)
 *   3. biquad Q30 inner (single-channel × WINDOW_SAMPLES)
 *   4. lifting 5/3 forward, 2500 samples, 3 levels
 *   5. LPC analyze: autocorrelation(256, order 8) + Levinson + residuals(2500)
 *
 * Compile:
 *   gcc -O2 -march=native -o bench_kernels_c bench_kernels_c.c
 *   gcc -O3 -march=native -o bench_kernels_c_o3 bench_kernels_c.c
 *
 * Same xorshift64* PRNG seed as the Rust bench so accumulators must match.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define WINDOW_SAMPLES 2500
#define NUM_CHANNELS   21
#define LPC_ORDER      8
#define LPC_AUTOCORR_LEN 256

/* ── Xorshift64* PRNG ────────────────────────────────────────────── */
static uint64_t prng_state = 0x4C4D5141DEADBEEFULL;
static inline uint64_t prng_next(void) {
    uint64_t x = prng_state;
    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
    prng_state = x;
    return x;
}
static void prng_reset(void) { prng_state = 0x4C4D5141DEADBEEFULL; }

/* ── Q-format helpers ────────────────────────────────────────────── */
static inline int32_t mul_q31(int32_t a, int32_t b) {
    return (int32_t)(((int64_t)a * (int64_t)b) >> 31);
}
static inline int32_t mul_q30(int32_t a, int32_t b) {
    return (int32_t)(((int64_t)a * (int64_t)b) >> 30);
}
static inline int32_t add_sat_q31(int32_t a, int32_t b) {
    int64_t r = (int64_t)a + (int64_t)b;
    if (r > INT32_MAX) return INT32_MAX;
    if (r < INT32_MIN) return INT32_MIN;
    return (int32_t)r;
}
static inline int32_t sub_sat_q31(int32_t a, int32_t b) {
    int64_t r = (int64_t)a - (int64_t)b;
    if (r > INT32_MAX) return INT32_MAX;
    if (r < INT32_MIN) return INT32_MIN;
    return (int32_t)r;
}

/* ── Ternary kernels ─────────────────────────────────────────────── */
static const int32_t TERNARY_LUT[4] = {0, 1, -1, 0};
static inline int32_t mac_byte_lut(uint8_t pw, const int16_t* a) {
    int32_t s = 0;
    s += (int32_t)a[0] * TERNARY_LUT[(pw     ) & 3];
    s += (int32_t)a[1] * TERNARY_LUT[(pw >> 2) & 3];
    s += (int32_t)a[2] * TERNARY_LUT[(pw >> 4) & 3];
    s += (int32_t)a[3] * TERNARY_LUT[(pw >> 6) & 3];
    return s;
}
__attribute__((always_inline)) static inline int32_t mac_byte_fast(uint8_t pw, const int16_t* a) {
    int32_t acc = 0;
    #define T(i) do { \
        uint32_t w = (pw >> ((i) * 2)) & 3; \
        int32_t  x = (int32_t)a[i]; \
        int32_t  neg = -(int32_t)(w >> 1); \
        int32_t  v = (x ^ neg) - neg; \
        uint32_t nz = (w & 1) ^ (w >> 1); \
        acc += v & (-(int32_t)nz); \
    } while (0)
    T(0); T(1); T(2); T(3);
    #undef T
    return acc;
}

/* ── Biquad Q30 DF1 ──────────────────────────────────────────────── */
typedef struct { int32_t b0,b1,b2,a1,a2,x1,x2,y1,y2; } biq_t;
static inline int32_t biq_proc(biq_t* S, int32_t x0) {
    int32_t y = mul_q30(S->b0, x0);
    y = add_sat_q31(y, mul_q30(S->b1, S->x1));
    y = add_sat_q31(y, mul_q30(S->b2, S->x2));
    y = sub_sat_q31(y, mul_q30(S->a1, S->y1));
    y = sub_sat_q31(y, mul_q30(S->a2, S->y2));
    S->x2 = S->x1; S->x1 = x0;
    S->y2 = S->y1; S->y1 = y;
    return y;
}
/* 0.5 Hz coefficients, prod default */
static const int32_t HP_05[5] = { 1064243069, -2128486138, 1064243069, -2128402106, 1054828345 };

/* ── Lifting 1D 5/3 in-place ─────────────────────────────────────── */
static void lifting_1d_53(int32_t* sig, int len) {
    if (len < 2) return;
    int n_d = len / 2;
    int n_a = (len + 1) / 2;
    for (int n = 0; n < n_d - 1; n++) {
        sig[2*n + 1] -= (sig[2*n] + sig[2*n + 2]) >> 1;
    }
    if (n_d > 0) {
        int lo = 2*(n_d - 1) + 1, le = 2*(n_d - 1);
        if (lo < len) {
            if (len % 2 == 0) sig[lo] -= (sig[le] + sig[le]) >> 1;
            else              sig[lo] -= (sig[le] + sig[lo + 1]) >> 1;
        }
    }
    sig[0] += (sig[1] + 1) >> 1;
    for (int n = 1; n < n_a; n++) {
        int li = 2*n - 1, ri = 2*n + 1;
        if (ri < len) {
            int32_t s = sig[li] + sig[ri];
            sig[2*n] += (s >= 0) ? (s + 2) >> 2 : -(((-s) + 2) >> 2);
        } else {
            sig[2*n] += (sig[li] + 1) >> 1;
        }
    }
}
/* Full 3-level lifting pipeline matching firmware/dsp/lifting_2d.c:
 *   level 1: lift 2500 → deinterleave → approx_l1[1250], detail_l1[1250]
 *   level 2: lift approx_l1 → deinterleave → approx_l2[625], detail_l2[625]
 *   level 3: lift approx_l2 → deinterleave → approx_l3[313], detail_l3[312]
 * This is the apples-to-apples bench against the Rust port.
 */
static void lifting_3level(int32_t* x,
                           int32_t* d1_out, int32_t* d2_out,
                           int32_t* a3_out, int32_t* d3_out,
                           int32_t* scratch_a, int32_t* scratch_b) {
    /* Level 1 */
    lifting_1d_53(x, 2500);
    for (int i = 0; i < 1250; i++) scratch_a[i] = x[2*i];
    for (int i = 0; i < 1250; i++) d1_out[i]   = x[2*i + 1];

    /* Level 2 */
    lifting_1d_53(scratch_a, 1250);
    for (int i = 0; i < 625; i++) scratch_b[i] = scratch_a[2*i];
    for (int i = 0; i < 625; i++) d2_out[i]    = scratch_a[2*i + 1];

    /* Level 3 */
    lifting_1d_53(scratch_b, 625);
    for (int i = 0; i < 313; i++) a3_out[i] = scratch_b[2*i];
    for (int i = 0; i < 312; i++) d3_out[i] = scratch_b[2*i + 1];
}

/* ── LPC analyze (autocorr + Levinson + residuals) ───────────────── */
static void autocorrelation(const int32_t* x, int len, int64_t* R, int order) {
    for (int k = 0; k <= order; k++) {
        int64_t acc = 0;
        int n = k;
        int end4 = k + ((len - k) & ~3);
        for (; n < end4; n += 4) {
            acc += (int64_t)x[n]     * (int64_t)x[n - k];
            acc += (int64_t)x[n + 1] * (int64_t)x[n + 1 - k];
            acc += (int64_t)x[n + 2] * (int64_t)x[n + 2 - k];
            acc += (int64_t)x[n + 3] * (int64_t)x[n + 3 - k];
        }
        for (; n < len; n++) acc += (int64_t)x[n] * (int64_t)x[n - k];
        R[k] = acc / len;
    }
}
static int levinson(const int64_t* R, int order, int32_t* a) {
    if (R[0] == 0) return 0;
    int64_t E = R[0];
    int32_t a_prev[16] = {0}, a_curr[16] = {0};
    for (int m = 0; m < order; m++) {
        int64_t sum = R[m + 1];
        for (int j = 0; j < m; j++) sum += ((int64_t)a_prev[j] * R[m - j]) >> 31;
        if (E == 0) return 0;
        int64_t k64 = -(sum / E) * (int64_t)(1LL << 31);
        if (k64 > INT32_MAX) k64 = INT32_MAX;
        if (k64 < INT32_MIN) k64 = INT32_MIN;
        int32_t k = (int32_t)k64;
        a_curr[m] = k;
        for (int j = 0; j < m; j++) a_curr[j] = add_sat_q31(a_prev[j], mul_q31(k, a_prev[m - 1 - j]));
        E -= ((int64_t)k * k64) / (1LL << 31);
        if (E <= 0) E = 1;
        for (int j = 0; j <= m; j++) a_prev[j] = a_curr[j];
    }
    for (int i = 0; i < order; i++) a[i] = a_curr[i];
    return 1;
}
static void lpc_resid_o8(const int32_t* x, int32_t* r, int len, const int32_t* a) {
    for (int n = 0; n < 8; n++) r[n] = x[n];
    for (int n = 8; n < len; n++) {
        int32_t p = mul_q31(a[0], x[n-1]);
        p = add_sat_q31(p, mul_q31(a[1], x[n-2]));
        p = add_sat_q31(p, mul_q31(a[2], x[n-3]));
        p = add_sat_q31(p, mul_q31(a[3], x[n-4]));
        p = add_sat_q31(p, mul_q31(a[4], x[n-5]));
        p = add_sat_q31(p, mul_q31(a[5], x[n-6]));
        p = add_sat_q31(p, mul_q31(a[6], x[n-7]));
        p = add_sat_q31(p, mul_q31(a[7], x[n-8]));
        r[n] = sub_sat_q31(x[n], p);
    }
}

static inline double ns_diff(struct timespec* a, struct timespec* b) {
    return (b->tv_sec - a->tv_sec) * 1e9 + (b->tv_nsec - a->tv_nsec);
}

int main(void) {
    struct timespec t0, t1;

    /* ─────────── 1. Ternary mac_byte ─────────── */
    const size_t N = 100000000UL;
    const size_t SZ = 1024 * 1024;
    int16_t* acts  = malloc(SZ * sizeof(int16_t));
    uint8_t* packs = malloc((SZ / 4) * sizeof(uint8_t));
    prng_reset();
    for (size_t i = 0; i < SZ; i++) acts[i] = (int16_t)(prng_next() & 0xFFFF);
    for (size_t i = 0; i < SZ / 4; i++) {
        uint64_t r = prng_next();
        uint8_t b = 0;
        for (int j = 0; j < 4; j++) {
            uint8_t w = (r >> (j * 8)) & 3;
            if (w == 3) w = 0;
            b |= (w << (j * 2));
        }
        packs[i] = b;
    }

    /* warmup */
    int32_t sink = 0;
    for (size_t i = 0; i < 1000; i++) sink += mac_byte_fast(packs[i & 0xFFFF], &acts[(i*4) & 0x3FFFC]);
    (void)sink;

    int32_t lut_acc = 0;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (size_t i = 0; i < N; i++) {
        size_t pi = i & ((SZ / 4) - 1);
        size_t ai = (i * 4) & (SZ - 4);
        lut_acc += mac_byte_lut(packs[pi], &acts[ai]);
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    double lut_ns = ns_diff(&t0, &t1);

    int32_t fast_acc = 0;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (size_t i = 0; i < N; i++) {
        size_t pi = i & ((SZ / 4) - 1);
        size_t ai = (i * 4) & (SZ - 4);
        fast_acc += mac_byte_fast(packs[pi], &acts[ai]);
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    double fast_ns = ns_diff(&t0, &t1);

    printf("=== C kernels (host x86, gcc) ===\n");
    printf("ternary LUT      : %.2f ns/byte    %.2f Gmac/s   acc=%d\n",
           lut_ns / N,  N * 4.0 / (lut_ns / 1e9) / 1e9, lut_acc);
    printf("ternary fast     : %.2f ns/byte    %.2f Gmac/s   acc=%d\n",
           fast_ns / N, N * 4.0 / (fast_ns / 1e9) / 1e9, fast_acc);

    /* ─────────── 2. Biquad — 21 ch × 2500 samples per call, 1000 windows ─────────── */
    static int32_t signal[NUM_CHANNELS][WINDOW_SAMPLES];
    prng_reset();
    for (int ch = 0; ch < NUM_CHANNELS; ch++)
        for (int i = 0; i < WINDOW_SAMPLES; i++)
            signal[ch][i] = (int32_t)(prng_next() & 0x3FFFFFFF) - 0x1FFFFFFF;

    biq_t st[NUM_CHANNELS];
    for (int ch = 0; ch < NUM_CHANNELS; ch++) {
        st[ch].b0 = HP_05[0]; st[ch].b1 = HP_05[1]; st[ch].b2 = HP_05[2];
        st[ch].a1 = HP_05[3]; st[ch].a2 = HP_05[4];
        st[ch].x1 = st[ch].x2 = st[ch].y1 = st[ch].y2 = 0;
    }
    int n_windows = 1000;
    int32_t biq_chk = 0;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (int w = 0; w < n_windows; w++) {
        for (int ch = 0; ch < NUM_CHANNELS; ch++) {
            biq_t* s = &st[ch];
            int32_t* row = signal[ch];
            for (int i = 0; i < WINDOW_SAMPLES; i++) row[i] = biq_proc(s, row[i]);
        }
        biq_chk ^= signal[0][1234];
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    double biq_ns = ns_diff(&t0, &t1);
    double biq_per_win = biq_ns / n_windows / 1000.0;
    long total_samples = (long)n_windows * NUM_CHANNELS * WINDOW_SAMPLES;
    printf("biquad 21x2500   : %.2f us/window  (%.0f Msamp/s, ns/sample %.2f, chk=%d)\n",
           biq_per_win,
           total_samples / (biq_ns / 1e9) / 1e6,
           biq_ns / total_samples, biq_chk);

    /* ─────────── 3. Lifting 3-level (full pipeline w/ deinterleave) ─────────── */
    static int32_t lift_in[WINDOW_SAMPLES];
    prng_reset();
    for (int i = 0; i < WINDOW_SAMPLES; i++) lift_in[i] = (int32_t)(prng_next() & 0x0FFFFFFF) - 0x07FFFFFF;
    int n_lift = 5000;
    int32_t lift_chk = 0;
    static int32_t buf[WINDOW_SAMPLES], scratch_a[1250], scratch_b[625];
    static int32_t d1[1250], d2[625], a3[313], d3[312];
    /* warmup */
    memcpy(buf, lift_in, sizeof(buf));
    lifting_3level(buf, d1, d2, a3, d3, scratch_a, scratch_b);

    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (int it = 0; it < n_lift; it++) {
        memcpy(buf, lift_in, sizeof(buf));
        lifting_3level(buf, d1, d2, a3, d3, scratch_a, scratch_b);
        lift_chk ^= a3[100] ^ d1[200] ^ d2[100] ^ d3[50];
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    double lift_ns = ns_diff(&t0, &t1);
    printf("lifting 3lvl 2500: %.2f us/call    chk=%d  (1ch full pipeline)\n",
           lift_ns / n_lift / 1000.0, lift_chk);
    printf("lifting 21ch est : %.2f us/window (extrapolated 21x 1ch)\n",
           lift_ns / n_lift / 1000.0 * 21);

    /* ─────────── 4. LPC analyze ─────────── */
    static int32_t lpc_in[WINDOW_SAMPLES];
    prng_reset();
    for (int i = 0; i < WINDOW_SAMPLES; i++) lpc_in[i] = (int32_t)(prng_next() & 0x00FFFFFF) - 0x007FFFFF;
    int n_lpc = 5000;
    int32_t lpc_chk = 0;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (int it = 0; it < n_lpc; it++) {
        int64_t R[LPC_ORDER + 1];
        int32_t a[LPC_ORDER];
        autocorrelation(lpc_in, LPC_AUTOCORR_LEN, R, LPC_ORDER);
        levinson(R, LPC_ORDER, a);
        static int32_t r[WINDOW_SAMPLES];
        lpc_resid_o8(lpc_in, r, WINDOW_SAMPLES, a);
        lpc_chk ^= r[2400];
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    double lpc_ns = ns_diff(&t0, &t1);
    printf("LPC analyze 2500 : %.2f us/call    chk=%d\n",
           lpc_ns / n_lpc / 1000.0, lpc_chk);

    free(acts); free(packs);
    return 0;
}
