// SPDX-License-Identifier: MPL-2.0
#define _GNU_SOURCE
#include <errno.h>
#include <dirent.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <unistd.h>
#include "sandbox.h"
#include "typedio.h"

struct instance { pid_t holder, carrier; const char *prefix; };
static void failure_logs(void)
{
    DIR *directory = opendir("/");
    if (!directory) return;
    struct dirent *entry;
    while ((entry = readdir(directory))) {
        if (!strstr(entry->d_name, ".console") && !strstr(entry->d_name, ".log")) continue;
        char path[512], data[4096];
        snprintf(path, sizeof(path), "/%s", entry->d_name);
        FILE *file = fopen(path, "r");
        if (!file) continue;
        printf("FAILURE LOG BEGIN %s\n", path);
        size_t size;
        while ((size = fread(data, 1, sizeof(data), file))) fwrite(data, 1, size, stdout);
        printf("\nFAILURE LOG END %s\n", path); fclose(file);
    }
    closedir(directory);
}
static void check(int ok, const char *what)
{
    if (ok) return;
    fprintf(stderr, "EXPERIMENT FAIL lifecycle %s errno=%d\n", what, errno);
    failure_logs();
    exit(1);
}
static void handoff(struct sandbox *sandbox, void *arg)
{
    int fd = *(int *)arg;
    check(write(fd, &sandbox->pids[0], sizeof(pid_t)) == sizeof(pid_t), "actual carrier PID handoff");
    close(fd);
}
static int holder(char **argv)
{
    struct sandbox_opts opt;
    char cgroup[128], console[128], log[128], cmdline[256];
    snprintf(cgroup, sizeof(cgroup), "/sys/fs/cgroup/%s", argv[5]);
    snprintf(console, sizeof(console), "/%s.console", argv[5]);
    snprintf(log, sizeof(log), "/%s.log", argv[5]);
    snprintf(cmdline, sizeof(cmdline), "console=hvc0 loglevel=warn -- %s %s 6000 %s", argv[2], argv[4], argv[3]);
    int control = atoi(argv[6]);
    sandbox_opts_init(&opt);
    opt.cgroup = cgroup; opt.console_path = console; opt.log_path = log;
    opt.pin = 1; opt.klet_kill_after_ms = 15000;
    const char *provider = getenv("KLET_TYPEDIO_FDS");
    check(provider != NULL, "inherited authorized provider");
    opt.typedio_fds[0] = atoi(provider); opt.nr_typedio_fds = 1;
    opt.during = handoff; opt.arg = &control;
    int system = open_system();
    int result = run_sandbox(system, "asterinas", 1, cmdline, &opt);
    close(system);
    return result;
}
static struct instance spawn(const char *role, const char *self, const char *peer,
                             const char *allowed, const char *prefix)
{
    int control[2];
    check(pipe2(control, O_CLOEXEC) == 0, "control pipe");
    pid_t child = fork();
    check(child >= 0, "holder fork");
    if (!child) {
        close(control[0]);
        check(fcntl(control[1], F_SETFD, 0) == 0, "carrier control inheritance");
        char value[32]; snprintf(value, sizeof(value), "%d", control[1]);
        execl("/typedio-device", "typedio-device", self, allowed, "--", "/typedio-lifecycle", "holder",
              role, self, peer, prefix, value, (char *)NULL);
        perror("exec lifecycle provider"); _exit(127);
    }
    close(control[1]);
    struct instance instance = {child, 0, prefix};
    check(read(control[0], &instance.carrier, sizeof(pid_t)) == sizeof(pid_t) && instance.carrier > 0, "real carrier identity");
    close(control[0]);
    return instance;
}
static int contains(const char *prefix, const char *marker)
{
    char path[128], data[65536];
    snprintf(path, sizeof(path), "/%s.console", prefix);
    FILE *file = fopen(path, "r");
    if (!file) return 0;
    size_t size = fread(data, 1, sizeof(data) - 1, file);
    fclose(file); data[size] = 0;
    return strstr(data, marker) != NULL;
}
static void wait_marker(struct instance instance, const char *marker)
{
    for (unsigned int i = 0; i < 1000; i++) {
        if (contains(instance.prefix, marker)) return;
        usleep(5000);
    }
    fprintf(stderr, "missing marker %s in %s\n", marker, instance.prefix);
    check(0, "tenant progress deadline");
}
static void stop(struct instance instance)
{
    printf("LIFECYCLE killing actual Guest Linux carrier pid=%d (%s)\n", instance.carrier, instance.prefix);
    check(kill(instance.carrier, SIGKILL) == 0, "real peer death");
    int status;
    check(waitpid(instance.holder, &status, 0) == instance.holder && WIFEXITED(status) && WEXITSTATUS(status) == 129, "holder reaped after carrier death");
}
static void dump(struct instance instance)
{
    for (int kind = 0; kind < 2; kind++) {
        char path[128], data[4096];
        snprintf(path, sizeof(path), "/%s.%s", instance.prefix, kind ? "log" : "console");
        FILE *file = fopen(path, "r");
        if (!file) continue;
        printf("LIFECYCLE LOG BEGIN %s\n", path);
        size_t size;
        while ((size = fread(data, 1, sizeof(data), file))) fwrite(data, 1, size, stdout);
        printf("\nLIFECYCLE LOG END %s\n", path); fclose(file);
    }
}
static int accept_host(int listener)
{
    struct pollfd pollfd = {listener, POLLIN, 0};
    check(poll(&pollfd, 1, 10000) == 1 && (pollfd.revents & POLLIN), "Host listener ready");
    int fd = ioctl(listener, KLET_TIO_ACCEPT, O_CLOEXEC);
    check(fd >= 0, "Host adapter accept"); return fd;
}
static void echo_host(int fd)
{
    unsigned char data[64];
    size_t done = 0;
    while (done < sizeof(data)) { ssize_t n = read(fd, data + done, sizeof(data) - done); check(n > 0, "Host lifecycle read"); done += n; }
    for (size_t i = 0; i < sizeof(data); i++) check(data[i] == (unsigned char)(0xa5 ^ i), "Host lifecycle contents");
    done = 0;
    while (done < sizeof(data)) { ssize_t n = write(fd, data + done, sizeof(data) - done); check(n > 0, "Host lifecycle write"); done += n; }
}
int main(int argc, char **argv)
{
    setbuf(stdout, NULL);
    if (argc == 7 && !strcmp(argv[1], "holder")) return holder(argv);
    check(argc == 2, "scenario argument");
    if (!strcmp(argv[1], "backlog")) {
        struct instance server = spawn("b", "7", "42", "42", "backlog-server");
        wait_marker(server, "BACKLOG SERVER READY");
        struct instance client = spawn("B", "42", "7", "7", "backlog-client");
        wait_marker(client, "BACKLOG CLIENT PASS all"); wait_marker(server, "BACKLOG SERVER PASS all");
        stop(client); stop(server); dump(client); dump(server);
        printf("LIFECYCLE BACKLOG PASS full-slot refusal, independent flow, accepted-stream recovery, listener close\n");
        return 0;
    }
    if (!strcmp(argv[1], "gh-reset")) {
        const char *value = getenv("KLET_TYPEDIO_LISTENER_FD");
        check(value != NULL, "Host listener capability");
        int listener = atoi(value);
        check(fcntl(listener, F_SETFD, FD_CLOEXEC) == 0, "Host listener exec boundary");
        struct instance first = spawn("X", "42", "2", "2", "gh-first");
        int fd = accept_host(listener); echo_host(fd);
        wait_marker(first, "GH LIFECYCLE CLIENT WAITING"); stop(first);
        unsigned char byte; errno = 0;
        check(read(fd, &byte, 1) == -1 && errno == ECONNRESET, "Host observes killed tenant reset");
        check(close(fd) == 0, "old Host flow release");
        struct instance next = spawn("X", "42", "2", "2", "gh-replacement");
        fd = accept_host(listener); echo_host(fd); check(close(fd) == 0, "replacement graceful close");
        wait_marker(next, "GH LIFECYCLE TENANT PASS all"); stop(next);
        close(listener); dump(first); dump(next);
        printf("LIFECYCLE HOST PASS peer death reset, same-CID replacement, useful surviving Host endpoint\n");
        return 0;
    }
    check(!strcmp(argv[1], "reset"), "known lifecycle scenario");
    struct instance server = spawn("R", "7", "42", "42", "reset-server");
    wait_marker(server, "RESET SERVER READY");
    struct instance denied = spawn("D", "42", "7", "2", "denied");
    wait_marker(denied, "LIFECYCLE ACL DENIAL PASS"); stop(denied);
    struct instance client = spawn("r", "42", "7", "7", "survivor");
    wait_marker(client, "RESET CLIENT WAITING"); wait_marker(server, "RESET SERVER HOLDING");
    stop(server); wait_marker(client, "RESET CLIENT PASS peer death");
    struct instance recovery = spawn("E", "7", "42", "42", "recovery-server");
    wait_marker(recovery, "RECOVERY SERVER READY");
    wait_marker(client, "LIFECYCLE CLIENT PASS all"); wait_marker(recovery, "RECOVERY SERVER PASS all");
    stop(client); stop(recovery);
    dump(denied); dump(server); dump(client); dump(recovery);
    printf("LIFECYCLE GUEST PASS authorization, real peer reset, same-CID generation replacement, survivor echo\n");
    return 0;
}
