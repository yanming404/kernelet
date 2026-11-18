// SPDX-License-Identifier: GPL-2.0
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include "../linux/endovisor/typedio_bounds.h"

int main(void)
{
	assert(klet_range_contains(0x1000, 0x4000, 0x1000, 0x4000));
	assert(klet_range_contains(0x1000, 0x4000, 0x2000, 0x1000));
	assert(!klet_range_contains(0x1000, 0x4000, 0x0fff, 1));
	assert(!klet_range_contains(0x1000, 0x4000, 0x5000, 1));
	assert(!klet_range_contains(0x1000, 0x4000, ULLONG_MAX - 1, 8));
	assert(!klet_range_contains(0x1000, 0x4000, 0x2000, ULLONG_MAX));
	assert(klet_range_contains(0x1000, 0x4000, 0x5000, 0));
	puts("TypedIO range checks: PASS");
	return 0;
}
