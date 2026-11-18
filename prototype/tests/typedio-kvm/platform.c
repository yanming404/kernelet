// SPDX-License-Identifier: MPL-2.0
// Real Asterinas userspace tests; no simulated registers or Host primitives.
#include "syscall.h"

enum { SET_GS=0x1001, SET_FS=0x1002, GET_FS=0x1003, GET_GS=0x1004 };
extern long experiment_thread(void (*fn)(void *), void *stack, void *arg);
extern void experiment_sigreturn(void);
extern void platform_set_xmm(const unsigned long *value);
extern void platform_get_xmm(unsigned long *value);
static unsigned char stacks[2][65536] __attribute__((aligned(4096)));
static unsigned long slots[2][4];
static int completed, ready, start, signal_seen, has_fsgsbase;

static unsigned long get_base(long which)
{
    unsigned long value = 0;
    require(sc2(__NR_arch_prctl, which, &value) == 0, "arch_prctl get");
    return value;
}

static void set_base(long which, unsigned long value)
{
    require(sc2(__NR_arch_prctl, which, value) == 0, "arch_prctl set");
}

static void check_registers(unsigned long *s, const unsigned long *pattern,
                            unsigned int mxcsr, unsigned short cw)
{
    unsigned long fs, gs, xmm[2];
    unsigned int got_mxcsr;
    unsigned short got_cw;
    platform_get_xmm(xmm);
    __asm__ volatile("stmxcsr %0; fnstcw %1" : "=m"(got_mxcsr), "=m"(got_cw));
    require(xmm[0] == pattern[0] && xmm[1] == pattern[1], "thread XMM isolation");
    require(got_mxcsr == mxcsr && got_cw == cw, "thread FPU control isolation");
    require(get_base(GET_FS) == (unsigned long)&s[0], "thread FS base isolation");
    require(get_base(GET_GS) == (unsigned long)&s[1], "thread GS base isolation");
    __asm__ volatile("mov %%fs:0, %0; mov %%gs:0, %1" : "=r"(fs), "=r"(gs));
    require(fs == s[0] && gs == s[1], "thread TLS contents");
}

static void worker(void *arg)
{
    unsigned long id = (unsigned long)arg;
    unsigned long *s = slots[id];
    unsigned long pattern[2] = {0x123456789abcdef0UL + id, 0xfedcba9876543210UL - id};
    unsigned int mxcsr = id ? 0x3f80 : 0x1f80;
    unsigned short cw = id ? 0x77f : 0x37f;
    s[0] = 0x1122334400000000UL + id;
    s[1] = 0x5566778800000000UL + id;
    set_base(SET_FS, (unsigned long)&s[0]);
    set_base(SET_GS, (unsigned long)&s[1]);
    __asm__ volatile("ldmxcsr %0; fldcw %1" :: "m"(mxcsr), "m"(cw));
    platform_set_xmm(pattern);
    __atomic_fetch_add(&ready, 1, __ATOMIC_SEQ_CST);
    while (!__atomic_load_n(&start, __ATOMIC_SEQ_CST)) sc0(__NR_sched_yield);
    for (unsigned int i = 0; i < 128; i++) {
        require(sc0(__NR_gettid) > 0, "thread gettid syscall");
        sc0(__NR_sched_yield);
        sleep_ms(1);
        check_registers(s, pattern, mxcsr, cw);
        if (has_fsgsbase) {
            unsigned long fs, gs;
            __asm__ volatile("rdfsbase %0; rdgsbase %1" : "=r"(fs), "=r"(gs));
            require(fs == (unsigned long)&s[0] && gs == (unsigned long)&s[1], "FSGSBASE read");
            fs = (unsigned long)&s[2]; gs = (unsigned long)&s[3];
            __asm__ volatile("wrfsbase %0; wrgsbase %1" :: "r"(fs), "r"(gs) : "memory");
            sc0(__NR_sched_yield);
            require(get_base(GET_FS) == fs && get_base(GET_GS) == gs, "FSGSBASE syscall synchronization");
            set_base(SET_FS, (unsigned long)&s[0]);
            set_base(SET_GS, (unsigned long)&s[1]);
        }
    }
    __atomic_fetch_add(&completed, 1, __ATOMIC_SEQ_CST);
}

static void signal_handler(int signal)
{
    unsigned long xmm[2], changed[2] = {0xabc, 0xdef};
    platform_get_xmm(xmm);
    require(signal == 10 && xmm[0] == 0 && xmm[1] == 0, "signal initial FPU state");
    platform_set_xmm(changed);
    signal_seen = 1;
}

int experiment_main(void)
{
    unsigned int a, b, c, d;
    __asm__ volatile("cpuid" : "=a"(a), "=b"(b), "=c"(c), "=d"(d) : "a"(7), "c"(0));
    has_fsgsbase = !!(b & 1);
    say("PLATFORM BEGIN real userspace TLS/FPU/scheduling\n");
    unsigned long old_fs = get_base(GET_FS), old_gs = get_base(GET_GS);
    require(sc2(__NR_arch_prctl, SET_FS, 0x0000800000000000UL) < 0, "noncanonical FS rejection");
    require(sc2(__NR_arch_prctl, SET_GS, 0x0000800000000000UL) < 0, "noncanonical GS rejection");
    require(get_base(GET_FS) == old_fs && get_base(GET_GS) == old_gs, "rejected base preserves TLS");
    say("PLATFORM PASS arch_prctl noncanonical rejection\n");
    if (has_fsgsbase) {
        unsigned long high = 0xffff800000001000UL;
        __asm__ volatile("wrfsbase %0; wrgsbase %0" :: "r"(high) : "memory");
        require(sc0(__NR_getpid) == 1, "PID1 syscall with canonical high TLS");
        require(get_base(GET_FS) == high && get_base(GET_GS) == high, "canonical high TLS survives syscall");
        set_base(SET_FS, old_fs); set_base(SET_GS, old_gs);
        say("PLATFORM PASS canonical high FS/GS through FSGSBASE and syscall\n");
    } else say("PLATFORM NOT RUN FSGSBASE CPU feature absent\n");
    for (unsigned long i = 0; i < 2; i++)
        require(experiment_thread(worker, stacks[i] + sizeof(stacks[i]), (void *)i) > 0, "clone thread");
    while (__atomic_load_n(&ready, __ATOMIC_SEQ_CST) != 2) sleep_ms(1);
    __atomic_store_n(&start, 1, __ATOMIC_SEQ_CST);
    for (unsigned int i = 0; __atomic_load_n(&completed, __ATOMIC_SEQ_CST) != 2; i++) {
        require(i < 5000, "thread progress deadline");
        sleep_ms(1);
    }
    require(get_base(GET_FS) == old_fs && get_base(GET_GS) == old_gs, "parent TLS isolation");
    say("PLATFORM PASS two threads FS/GS XMM MXCSR x87 control 128 blocking/yield iterations each\n");
    struct {
        void (*handler)(int);
        unsigned long flags;
        void (*restorer)(void);
        unsigned long mask;
    } action = {signal_handler, 0x04000000UL, experiment_sigreturn, 0};
    require(sc6(__NR_rt_sigaction, 10, (long)&action, 0, 8, 0, 0) == 0, "rt_sigaction");
    unsigned long pattern[2] = {0xcafebabecafebabeUL, 0xdeadbeefdeadbeefUL}, xmm[2];
    platform_set_xmm(pattern);
    require(sc3(__NR_tgkill, sc0(__NR_getpid), sc0(__NR_gettid), 10) == 0, "tgkill signal");
    platform_get_xmm(xmm);
    require(signal_seen && xmm[0] == pattern[0] && xmm[1] == pattern[1], "rt_sigreturn FPU restoration");
    say("PLATFORM PASS signal FPU initialization and rt_sigreturn restoration\n");
    say("PLATFORM PASS all\n");
    return 0;
}
