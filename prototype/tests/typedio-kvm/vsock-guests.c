// SPDX-License-Identifier: MPL-2.0
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static void check(int ok, const char *what)
{
    if (ok) return;
    fprintf(stderr, "EXPERIMENT FAIL Guest pair %s\n", what);
    exit(1);
}
static pid_t spawn(int server)
{
    pid_t child = fork();
    check(child >= 0, "fork");
    if (!child) {
        execl("/typedio-device", "typedio-device", server ? "7" : "42", server ? "42" : "7", "--",
              "/holder", "run", "-p", "-c", server ? "/sys/fs/cgroup/server" : "/sys/fs/cgroup/client",
              "--console", server ? "/server.console" : "/client.console", "--log", server ? "/server.log" : "/client.log",
              "--klet-kill-after", "15000", "asterinas", "1", "console=hvc0", "loglevel=warn", "--",
              server ? "S" : "C", server ? "42" : "7", "8000", server ? "7" : "42", (char *)NULL);
        perror("exec provisioning"); _exit(127);
    }
    return child;
}
static int contains(const char *path, const char *marker)
{
    char data[65536];
    FILE *file = fopen(path, "r");
    if (!file) return 0;
    size_t size = fread(data, 1, sizeof(data) - 1, file);
    fclose(file); data[size] = 0;
    return strstr(data, marker) != NULL;
}
static void dump(const char *path)
{
    char data[4096];
    FILE *file = fopen(path, "r");
    if (!file) return;
    printf("GUEST LOG BEGIN %s\n", path);
    size_t size;
    while ((size = fread(data, 1, sizeof(data), file))) fwrite(data, 1, size, stdout);
    printf("\nGUEST LOG END %s\n", path);
    fclose(file);
}
int main(void)
{
    setbuf(stdout, NULL);
    pid_t server = spawn(1);
    unsigned int i;
    for (i = 0; i < 1000 && !contains("/server.console", "SMOKE SERVER READY"); i++) usleep(5000);
    check(i < 1000, "real server readiness deadline");
    pid_t client = spawn(0);
    int first, second;
    check(waitpid(client, &first, 0) == client && waitpid(server, &second, 0) == server, "holders reaped");
    dump("/server.console"); dump("/server.log"); dump("/client.console"); dump("/client.log");
    check(WIFEXITED(first) && WEXITSTATUS(first) == 129 && WIFEXITED(second) && WEXITSTATUS(second) == 129, "controlled instance teardown");
    check(contains("/server.console", "SMOKE SERVER PASS all") && contains("/client.console", "SMOKE TENANT PASS all"), "both complete Asterinas applications passed");
    printf("SMOKE GUEST-GUEST PASS two complete instances, four concurrent flows\n");
    return 0;
}
