// SPDX-License-Identifier: GPL-2.0
#ifndef _KLET_TYPEDIO_BOUNDS_H
#define _KLET_TYPEDIO_BOUNDS_H

// Validate by subtraction so neither an address nor extent addition can wrap.
// This common helper is compiled by both Kbuild and the host-native C test.
static inline int klet_range_contains(unsigned long long base,
				     unsigned long long extent,
				     unsigned long long address,
				     unsigned long long length)
{
	return address >= base && length <= extent &&
	       address - base <= extent - length;
}

#endif
