/* SPDX-License-Identifier: GPL-2.0 */
#include <assert.h>
#include <stdint.h>
#include "../linux/endovisor/platform_tls.h"
int main(void)
{
    const uint64_t limit = UINT64_C(1) << 47;
    assert(klet_tls_request_valid(0, 0, UINT64_MAX, limit, 48, 0));
    assert(klet_tls_request_valid(1, 0, 0, limit, 48, 0));
    assert(klet_tls_request_valid(0, 1, 0, limit, 48, 0));
    assert(klet_tls_request_valid(1, 1, limit - 1, limit, 48, 0));
    assert(!klet_tls_request_valid(0, 1, limit, limit, 48, 0));
    assert(!klet_tls_request_valid(1, 1, UINT64_MAX, limit, 48, 0));
    assert(!klet_tls_request_valid(2, 0, 0, limit, 48, 0));
    assert(!klet_tls_request_valid(UINT64_MAX, 0, 0, limit, 48, 0));
    assert(!klet_tls_request_valid(0, 2, 0, limit, 48, 0));
    for (unsigned int which = 0; which < 2; ++which) {
        assert(klet_tls_request_valid(which, 1, UINT64_MAX, limit, 48, 1));
        assert(klet_tls_request_valid(which, 1, UINT64_C(0xffff800000000000), limit, 48, 1));
        assert(!klet_tls_request_valid(which, 1, UINT64_C(0x0000800000000000), limit, 48, 1));
        assert(!klet_tls_request_valid(which, 1, UINT64_C(0xffff7fffffffffff), limit, 48, 1));
        assert(klet_tls_request_valid(which, 1, UINT64_C(0xff00000000000000), limit, 57, 1));
        assert(!klet_tls_request_valid(which, 1, UINT64_C(0x0100000000000000), limit, 57, 1));
    }
    assert(!klet_tls_request_valid(0, 1, 0, limit, 64, 1));
    assert(klet_tls_scalar_in_stack(0x1000, 0x1000, 4096));
    assert(klet_tls_scalar_in_stack(0x1ff8, 0x1000, 4096));
    assert(!klet_tls_scalar_in_stack(0x1ffc, 0x1000, 4096));
    assert(!klet_tls_scalar_in_stack(0x2000, 0x1000, 4096));
    assert(!klet_tls_scalar_in_stack(0xff8, 0x1000, 4096));
    assert(!klet_tls_scalar_in_stack(0x1000, 0x1000, 7));
    assert(!klet_tls_scalar_in_stack(0, UINT64_MAX - 4095, 4096));
    return 0;
}
