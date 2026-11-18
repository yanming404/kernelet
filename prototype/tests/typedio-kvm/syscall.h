// SPDX-License-Identifier: MPL-2.0
#pragma once
#include <asm/unistd.h>
#include <stddef.h>
#include <stdint.h>

static inline long sc6(long nr, long a, long b, long c, long d, long e, long f)
{
    register long r10 __asm__("r10") = d;
    register long r8 __asm__("r8") = e;
    register long r9 __asm__("r9") = f;
    long result;
    __asm__ volatile("syscall" : "=a"(result)
        : "a"(nr), "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9)
        : "rcx", "r11", "memory");
    return result;
}
#define sc0(n) sc6(n, 0, 0, 0, 0, 0, 0)
#define sc1(n,a) sc6(n, (long)(a), 0, 0, 0, 0, 0)
#define sc2(n,a,b) sc6(n, (long)(a), (long)(b), 0, 0, 0, 0)
#define sc3(n,a,b,c) sc6(n, (long)(a), (long)(b), (long)(c), 0, 0, 0)

static void say(const char *text)
{
    size_t len = 0;
    while (text[len]) len++;
    while (len) {
        long n = sc3(__NR_write, 1, text, len);
        if (n <= 0) sc1(__NR_exit_group, 90);
        text += n;
        len -= (size_t)n;
    }
}

static void require(int ok, const char *name)
{
    if (ok) return;
    say("EXPERIMENT FAIL "); say(name); say("\n");
    sc1(__NR_exit_group, 91);
    for (;;) {}
}

static inline void sleep_ms(long ms)
{
    long time[2] = {ms / 1000, (ms % 1000) * 1000000};
    require(sc2(__NR_nanosleep, time, 0) == 0, "nanosleep");
}

static inline void say_number(const char *prefix, long value)
{
    char line[128], digits[32];
    unsigned int used = 0, count = 0;
    while (*prefix && used < sizeof(line) - 40) line[used++] = *prefix++;
    unsigned long magnitude = value < 0 ? (unsigned long)(-(value + 1)) + 1 : (unsigned long)value;
    if (value < 0) line[used++] = '-';
    do { digits[count++] = '0' + magnitude % 10; magnitude /= 10; } while (magnitude);
    while (count) line[used++] = digits[--count];
    line[used++] = '\n'; line[used] = 0; say(line);
}
