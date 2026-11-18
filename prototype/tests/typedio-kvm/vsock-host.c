// SPDX-License-Identifier: MPL-2.0
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <unistd.h>
#include "typedio.h"

static void check(int ok, const char *what)
{
    if (ok) return;
    fprintf(stderr, "EXPERIMENT FAIL Host %s errno=%d\n", what, errno);
    exit(1);
}
static void full(int fd, unsigned char *data, size_t size, int writing)
{
    size_t done = 0;
    while (done < size) {
        ssize_t n = writing ? write(fd, data + done, size - done) : read(fd, data + done, size - done);
        check(n > 0 && (size_t)n <= size - done, "stream prefix");
        done += n;
    }
}
static void *echo(void *arg)
{
    int stream = (int)(intptr_t)arg;
    unsigned char *data = malloc(131089);
    check(data != NULL, "echo allocation");
    unsigned int identity;
    full(stream, (unsigned char *)&identity, sizeof(identity), 0);
    check(identity >= 1 && identity <= 4, "connection identity");
    full(stream, (unsigned char *)&identity, sizeof(identity), 1);
    for (unsigned int round = 0; round < 9; round++) {
        size_t size = round == 8 ? 131089 : 4093;
        if (round == 8) usleep(50000);
        full(stream, data, size, 0);
        for (size_t i = 0; i < size; i++) check(data[i] == (unsigned char)(i * 17 + round * 31 + identity * 13 + 3), "payload integrity");
        full(stream, data, size, 1);
    }
    unsigned char byte;
    check(read(stream, &byte, 1) == 0, "tenant half-close EOF");
    check(close(stream) == 0, "adapter endpoint close");
    free(data);
    return NULL;
}
int main(void)
{
    setbuf(stdout, NULL);
    const char *value = getenv("KLET_TYPEDIO_LISTENER_FD");
    check(value != NULL, "real inherited listener FD");
    int listener = atoi(value);
    check(fcntl(listener, F_SETFD, FD_CLOEXEC) == 0, "listener inheritance boundary");
    pid_t child = fork();
    check(child >= 0, "fork holder");
    if (!child) {
        execl("/typedio-device", "typedio-device", "42", "2", "--", "/holder", "run", "-k", "-p", "-c",
              "/sys/fs/cgroup/sandbox", "--klet-kill-after", "15000", "asterinas", "1",
              "console=hvc0", "loglevel=warn", "--", "C", "2", "8000", "42", (char *)NULL);
        perror("exec typedio-device"); _exit(127);
    }
    pthread_t threads[4];
    for (unsigned int i = 0; i < 4; i++) {
        struct pollfd pollfd = {listener, POLLIN, 0};
        check(poll(&pollfd, 1, 10000) == 1 && (pollfd.revents & POLLIN), "listener readiness deadline");
        int stream = ioctl(listener, KLET_TIO_ACCEPT, O_CLOEXEC);
        check(stream >= 0, "real adapter accept");
        check(pthread_create(&threads[i], NULL, echo, (void *)(intptr_t)stream) == 0, "concurrent Host echo");
    }
    printf("SMOKE HOST Established four concurrent custom File Adapter connections\n");
    check(close(listener) == 0, "listener close preserves accepted streams");
    for (unsigned int i = 0; i < 4; i++) check(pthread_join(threads[i], NULL) == 0, "Host echo completion");
    int status;
    check(waitpid(child, &status, 0) == child && WIFEXITED(status) && WEXITSTATUS(status) == 129, "controlled holder teardown");
    printf("SMOKE HOST PASS all\n");
    return 0;
}
