// SPDX-License-Identifier: MPL-2.0
#include "syscall.h"
extern int platform_main(void);

struct address { uint16_t family, reserved; uint32_t port, cid; unsigned char zero[4]; };
struct pollfd { int fd; short events, revents; };
struct event { uint32_t events; uint64_t data; } __attribute__((packed));
static unsigned char buffers[4][131089];
static unsigned char stacks[4][65536] __attribute__((aligned(4096)));
extern long experiment_thread(void (*fn)(void *), void *stack, void *arg);
struct connection { int fd; unsigned int index, server; };
static struct connection connections[4];
static unsigned int partials, blocked_writes;
static unsigned int completed;

static unsigned int number(const char *s)
{
    unsigned int value = 0;
    while (*s) { require(*s >= '0' && *s <= '9', "numeric argument"); value = value * 10 + *s++ - '0'; }
    return value;
}

static void wait_fd(int fd, int events)
{
    struct pollfd poll = {fd, events, 0};
    require(sc3(__NR_poll, &poll, 1, 5000) == 1, "poll progress deadline");
    require(poll.revents & (events | 8 | 16 | 0x2000), "poll readiness");
}

static void full(int fd, unsigned char *data, size_t size, int writing)
{
    while (size) {
        long n = sc3(writing ? __NR_write : __NR_read, fd, data, size);
        if (n == -11) {
            if (writing) __atomic_fetch_add(&blocked_writes, 1, __ATOMIC_RELAXED);
            wait_fd(fd, writing ? 4 : 1);
            continue;
        }
        require(n > 0 && (size_t)n <= size, "complete stream prefix");
        if ((size_t)n < size) __atomic_fetch_add(&partials, 1, __ATOMIC_RELAXED);
        data += n; size -= (size_t)n;
    }
}

static unsigned char pattern(size_t byte, unsigned int round, unsigned int identity)
{
    return (unsigned char)(byte * 17 + round * 31 + identity * 13 + 3);
}

static void worker(void *arg)
{
    struct connection *connection = arg;
    unsigned char *data = buffers[connection->index];
    int fd = connection->fd;
    unsigned int identity = connection->index + 1;
    if (connection->server) {
        full(fd, (unsigned char *)&identity, sizeof(identity), 0);
        require(identity >= 1 && identity <= 4, "unique connection identity");
        full(fd, (unsigned char *)&identity, sizeof(identity), 1);
        for (unsigned int round = 0; round < 9; round++) {
            size_t size = round == 8 ? sizeof(buffers[0]) : 4093;
            if (round == 8) sleep_ms(50);
            full(fd, data, size, 0);
            for (size_t i = 0; i < size; i++) require(data[i] == pattern(i, round, identity), "server payload identity");
            full(fd, data, size, 1);
        }
        unsigned char byte;
        require(sc3(__NR_read, fd, &byte, 1) == 0, "server half-close EOF");
        require(sc1(__NR_close, fd) == 0, "server connection close");
        __atomic_fetch_add(&completed, 1, __ATOMIC_SEQ_CST);
        return;
    }
    long flags = sc3(__NR_fcntl, fd, 3, 0);
    require(flags >= 0 && sc3(__NR_fcntl, fd, 4, flags | 0x800) == 0, "nonblocking flags");
    unsigned char one;
    require(sc3(__NR_read, fd, &one, 1) == -11, "empty nonblocking read EAGAIN");
    wait_fd(fd, 4);
    full(fd, (unsigned char *)&identity, sizeof(identity), 1);
    unsigned int reply = 0;
    full(fd, (unsigned char *)&reply, sizeof(reply), 0);
    require(reply == identity, "connection identity echo");
    int ep = sc1(__NR_epoll_create1, 0x80000);
    require(ep >= 0, "epoll create");
    struct event event = {1U | (1U << 31), 0x55};
    require(sc6(__NR_epoll_ctl, ep, 1, fd, (long)&event, 0, 0) == 0, "edge epoll registration");
    for (unsigned int round = 0; round < 9; round++) {
        size_t size = round == 8 ? sizeof(buffers[0]) : 4093;
        for (size_t i = 0; i < size; i++) data[i] = pattern(i, round, identity);
        full(fd, data, size, 1);
        struct event ready;
        require(sc6(__NR_epoll_wait, ep, (long)&ready, 1, 5000, 0, 0) == 1, "edge epoll delivery");
        require((ready.events & 1) && ready.data == 0x55, "epoll event identity");
        require(sc6(__NR_recvfrom, fd, (long)&one, 1, 2, 0, 0) == 1 && one == pattern(0, round, identity), "peek without consumption");
        full(fd, data, size, 0);
        for (size_t i = 0; i < size; i++) require(data[i] == pattern(i, round, identity), "echo payload integrity");
    }
    require(sc3(__NR_fcntl, fd, 4, flags & ~0x800L) == 0, "restore blocking mode");
    require(sc2(__NR_shutdown, fd, 1) == 0, "half-close send");
    require(sc3(__NR_read, fd, &one, 1) == 0, "blocking EOF after peer close");
    require(sc1(__NR_close, fd) == 0 && sc1(__NR_close, ep) == 0, "close socket and epoll");
    __atomic_fetch_add(&completed, 1, __ATOMIC_SEQ_CST);
}

static int listener_at(unsigned int self, unsigned int port, int backlog)
{
    struct address address = {40, 0, port, self, {0}};
    int fd = sc3(__NR_socket, 40, 1, 0);
    require(fd >= 0 && sc3(__NR_bind, fd, &address, sizeof(address)) == 0, "lifecycle bind");
    require(sc2(__NR_listen, fd, backlog) == 0, "lifecycle listen");
    return fd;
}

static int connect_to(unsigned int self, unsigned int peer, unsigned int port, unsigned int local, long *result)
{
    struct address address = {40, 0, local, self, {0}};
    struct address remote = {40, 0, port, peer, {0}};
    int fd = sc3(__NR_socket, 40, 1, 0);
    require(fd >= 0 && sc3(__NR_bind, fd, &address, sizeof(address)) == 0, "lifecycle client bind");
    *result = sc3(__NR_connect, fd, &remote, sizeof(remote));
    return fd;
}

static void small_echo(int fd, int server)
{
    unsigned char *data = buffers[0];
    if (!server) for (unsigned int i = 0; i < 64; i++) data[i] = (unsigned char)(0xa5 ^ i);
    full(fd, data, 64, !server);
    full(fd, data, 64, server);
    for (unsigned int i = 0; i < 64; i++) require(data[i] == (unsigned char)(0xa5 ^ i), "lifecycle echo contents");
}

static int lifecycle(char role, unsigned int self, unsigned int peer, unsigned int port)
{
    long result;
    int fd, listener;
    unsigned char byte;
    if (role == 'D') {
        fd = connect_to(self, peer, port, 5200, &result);
        require(result == -101, "real peer policy ENETUNREACH");
        sc1(__NR_close, fd);
        say("LIFECYCLE ACL DENIAL PASS existing peer forbidden\n");
        return 0;
    }
    if (role == 'R' || role == 'E') {
        listener = listener_at(self, port, 8);
        say(role == 'R' ? "RESET SERVER READY\n" : "RECOVERY SERVER READY\n");
        fd = sc3(__NR_accept, listener, 0, 0);
        require(fd >= 0, "reset/recovery accept");
        small_echo(fd, 1);
        if (role == 'R') { say("RESET SERVER HOLDING\n"); return 0; }
        require(sc1(__NR_close, listener) == 0 && sc1(__NR_close, fd) == 0, "recovery server close");
        say("RECOVERY SERVER PASS all\n"); return 0;
    }
    if (role == 'r' || role == 'X') {
        fd = connect_to(self, peer, port, 5201, &result);
        require(result == 0, "lifecycle connection established");
        small_echo(fd, 0);
        say(role == 'r' ? "RESET CLIENT WAITING\n" : "GH LIFECYCLE CLIENT WAITING\n");
        result = sc3(__NR_read, fd, &byte, 1);
        if (role == 'X') {
            require(result == 0, "Host close EOF on replacement flow");
            sc1(__NR_close, fd);
            say("GH LIFECYCLE TENANT PASS all\n"); return 0;
        }
        say_number("RESET observed_read=", result);
        require(result == -104, "real peer death ECONNRESET");
        sc1(__NR_close, fd);
        say("RESET CLIENT PASS peer death\n");
        for (unsigned int attempt = 0; ; attempt++) {
            require(attempt < 500, "survivor reconnect deadline");
            fd = connect_to(self, peer, port, 5500 + attempt, &result);
            if (result == 0) break;
            sc1(__NR_close, fd); sleep_ms(10);
        }
        small_echo(fd, 0);
        require(sc3(__NR_read, fd, &byte, 1) == 0, "recovery EOF");
        sc1(__NR_close, fd);
        say("LIFECYCLE CLIENT PASS all\n"); return 0;
    }
    if (role == 'b') {
        listener = listener_at(self, port, 1);
        int control = listener_at(self, port + 1, 1);
        say("BACKLOG SERVER READY\n");
        int gate = sc3(__NR_accept, control, 0, 0);
        require(gate >= 0 && sc3(__NR_read, gate, &byte, 1) == 1 && byte == 'G', "explicit backlog accept gate");
        sc1(__NR_close, gate); sc1(__NR_close, control);
        fd = sc3(__NR_accept, listener, 0, 0);
        require(fd >= 0, "admitted backlog connection");
        sc1(__NR_close, listener);
        small_echo(fd, 1); sc1(__NR_close, fd);
        say("BACKLOG SERVER PASS all\n"); return 0;
    }
    require(role == 'B', "known lifecycle role");
    fd = connect_to(self, peer, port, 5300, &result);
    require(result == 0, "first backlog slot admitted");
    int refused = connect_to(self, peer, port, 5301, &result);
    require(result == -104, "full backlog refuses excess flow with Reset");
    sc1(__NR_close, refused);
    int gate = connect_to(self, peer, port + 1, 5302, &result);
    require(result == 0, "independent control connection progresses");
    byte = 'G'; require(sc3(__NR_write, gate, &byte, 1) == 1, "release backlog gate");
    require(sc2(__NR_shutdown, gate, 1) == 0 && sc3(__NR_read, gate, &byte, 1) == 0, "control half-close");
    sc1(__NR_close, gate);
    small_echo(fd, 0);
    require(sc3(__NR_read, fd, &byte, 1) == 0, "accepted flow survives listener close");
    sc1(__NR_close, fd);
    refused = connect_to(self, peer, port, 5303, &result);
    require(result == -104, "closed listener refuses new connect with Reset");
    sc1(__NR_close, refused);
    say("BACKLOG CLIENT PASS all\n"); return 0;
}

int experiment_main(int argc, char **argv)
{
    if (argc == 2 && argv[1][0] == 'p') return platform_main();
    require(argc == 5, "init arguments: role peer port self");
    unsigned int peer = number(argv[2]), port = number(argv[3]), self = number(argv[4]);
    char role = argv[1][0];
    if (role == 'D' || role == 'R' || role == 'E' || role == 'r' || role == 'X' || role == 'b' || role == 'B')
        return lifecycle(role, self, peer, port);
    unsigned int server = argv[1][0] == 's' || argv[1][0] == 'S';
    unsigned int count = argv[1][0] == 'S' || argv[1][0] == 'C' ? 4 : 1;
    int listener = -1;
    if (server) {
        struct address local = {40, 0, port, self, {0}};
        listener = sc3(__NR_socket, 40, 1, 0);
        require(listener >= 0 && sc3(__NR_bind, listener, &local, sizeof(local)) == 0, "server socket bind");
        require(sc2(__NR_listen, listener, 8) == 0, "bounded server listen");
        say("SMOKE SERVER READY\n");
    }
    for (unsigned int i = 0; i < count; i++) {
        int fd;
        if (server) {
            wait_fd(listener, 1);
            fd = sc3(__NR_accept, listener, 0, 0);
            require(fd >= 0, "server accept");
        } else {
            struct address local = {40, 0, 5000 + i, self, {0}};
            struct address remote = {40, 0, port, peer, {0}};
            fd = sc3(__NR_socket, 40, 1, 0);
            require(fd >= 0, "AF_VSOCK socket");
            require(sc3(__NR_bind, fd, &local, sizeof(local)) == 0, "AF_VSOCK bind local CID");
            require(sc3(__NR_connect, fd, &remote, sizeof(remote)) == 0, "TypedIO connect/Established");
        }
        connections[i] = (struct connection){fd, i, server};
        require(experiment_thread(worker, stacks[i] + sizeof(stacks[i]), &connections[i]) > 0, "concurrent socket thread");
    }
    if (server) require(sc1(__NR_close, listener) == 0, "listener close preserves accepted streams");
    for (unsigned int i = 0; __atomic_load_n(&completed, __ATOMIC_SEQ_CST) != count; i++) {
        require(i < 10000, "concurrent connection deadline");
        sleep_ms(1);
    }
    if (server) { say("SMOKE SERVER PASS all\n"); return 0; }
    require(partials > 0 && blocked_writes > 0, "observed partial I/O and full-buffer backpressure");
    say("SMOKE TENANT PASS echo wrap partial nonblocking peek poll epoll backpressure concurrent connections\n");
    say("SMOKE TENANT PASS all\n");
    return 0;
}
