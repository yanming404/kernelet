/* SPDX-License-Identifier: GPL-2.0 */
#ifndef KLET_PLATFORM_TLS_H
#define KLET_PLATFORM_TLS_H
// The synchronous scalar must be wholly inside a live instance-owned stack.
static inline int klet_tls_scalar_in_stack(unsigned long long address,
                                           unsigned long long base,
                                           unsigned long long size)
{
    return !(address & 7) && address >= base && size >= 8 &&
           address - base <= size - 8;
}

// Validation shared with the executed native C regression.
// Reading ignores the input scalar.
// FSGSBASE permits any canonical base, including the high half: the value does
// not grant access to kernel memory. Without it, retain Linux arch_prctl bounds.
static inline int klet_tls_request_valid(unsigned long long which,
                                        unsigned long long operation,
                                        unsigned long long value,
                                        unsigned long long user_limit,
                                        unsigned int address_bits,
                                        int fsgsbase_enabled)
{
    unsigned long long upper;

    if (which > 1 || operation > 1)
        return 0;
    if (operation == 0)
        return 1;
    if (address_bits != 48 && address_bits != 57)
        return 0;
    upper = value >> (address_bits - 1);
    if (upper != 0 && upper != (~0ULL >> (address_bits - 1)))
        return 0;
    return fsgsbase_enabled || value < user_limit;
}
#endif
