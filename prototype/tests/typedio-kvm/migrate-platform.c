// SPDX-License-Identifier: MPL-2.0
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "sandbox.h"

struct observations { unsigned int seen, moves; int failed; };

static int last_cpu(pid_t pid)
{
    char path[80], line[4096], *save, *token;
    snprintf(path, sizeof(path), "/proc/%d/stat", pid);
    FILE *file = fopen(path, "r");
    if (!file) return -1;
    char *result = fgets(line, sizeof(line), file);
    fclose(file);
    if (!result) return -1;
    char *end = strrchr(line, ')');
    if (!end) return -1;
    token = strtok_r(end + 1, " ", &save);
    for (int field = 3; token; field++, token = strtok_r(NULL, " ", &save))
        if (field == 39) return atoi(token);
    return -1;
}

static void migrate(struct sandbox *sandbox, void *arg)
{
    struct observations *observed = arg;
    sandbox_say("MIGRATION START real carrier pid=%d", sandbox->pids[0]);
    msleep(10);
    for (unsigned int i = 0; i < 200; i++) {
        cpu_set_t mask;
        CPU_ZERO(&mask);
        CPU_SET(i % 2, &mask);
        if (sched_setaffinity(sandbox->pids[0], sizeof(mask), &mask)) {
            observed->failed = 1;
            break;
        }
        observed->moves++;
        msleep(2);
        int cpu = last_cpu(sandbox->pids[0]);
        if (cpu == 0 || cpu == 1) observed->seen |= 1U << cpu;
    }
    sandbox_say("MIGRATION observed_mask=%u successful_moves=%u", observed->seen, observed->moves);
}

int main(void)
{
    struct observations observed = {0};
    struct sandbox_opts opt;
    sandbox_who = "PLATFORM-HOST";
    sandbox_opts_init(&opt);
    opt.cgroup = "/sys/fs/cgroup/sandbox";
    opt.kernel_log = 1;
    opt.cpu_lo = 0;
    opt.cpu_hi = 1;
    opt.klet_kill_after_ms = 15000;
    opt.during = migrate;
    opt.arg = &observed;
    int system = open_system();
    int status = run_sandbox(system, "asterinas", 1, "console=hvc0 loglevel=warn -- platform", &opt);
    if (status != 129 || observed.failed || observed.seen != 3 || observed.moves != 200) {
        sandbox_say("EXPERIMENT FAIL carrier migration: status=%d mask=%u moves=%u", status, observed.seen, observed.moves);
        return 1;
    }
    sandbox_say("MIGRATION PASS real carrier observed on Guest Linux CPU0 and CPU1, 200 affinity changes");
    return 0;
}
