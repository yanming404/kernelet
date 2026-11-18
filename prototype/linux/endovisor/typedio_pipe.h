// SPDX-License-Identifier: GPL-2.0
#ifndef _KLET_TYPEDIO_PIPE_H
#define _KLET_TYPEDIO_PIPE_H

#include <linux/types.h>
#include "typedio_records.h"

// Host-private pipe storage; a numeric address never grants access to it.
struct klet_tio_pipe;
struct klet_tio_pipe_account;

u64 klet_tio_pipe_account_budget(u64 capacity, u32 pipes);
struct klet_tio_pipe_account *klet_tio_pipe_account_create(u64 owner,
						       u64 byte_limit, u32 pipe_limit);
void klet_tio_pipe_account_put(struct klet_tio_pipe_account *account);
struct klet_tio_pipe *klet_tio_pipe_create(struct klet_tio_pipe_account *account,
					 u64 producer, u64 consumer, u64 capacity);
void klet_tio_pipe_get(struct klet_tio_pipe *pipe);
void klet_tio_pipe_put(struct klet_tio_pipe *pipe);

// Reserve retains backing until the reservation is finalized.
// Payload access happens outside the metadata lock using a bounded opaque
// byte-copy adapter; safe Rust must never receive a shared payload slice.
int klet_tio_pipe_reserve(struct klet_tio_pipe *pipe, u64 owner, u32 role,
			 u64 limit, struct klet_tio_reservation *reservation);
int klet_tio_pipe_progress(struct klet_tio_pipe *pipe, u64 owner, u32 role,
			  u64 token, u64 bytes);
int klet_tio_pipe_commit(struct klet_tio_pipe *pipe, u64 owner, u32 role,
			u64 token, u64 bytes);
int klet_tio_pipe_cancel(struct klet_tio_pipe *pipe, u64 owner, u32 role, u64 token);
bool klet_tio_pipe_quiescent(struct klet_tio_pipe *pipe);
void klet_tio_pipe_close(struct klet_tio_pipe *pipe, u32 operation);

// Host application copies report both a transferred prefix and an optional
// fault. The caller commits that prefix (or zero for peek) after this returns.
long klet_tio_pipe_copy_user(struct klet_tio_pipe *pipe, u64 owner, u32 role,
                            u64 token, void __user *buffer, u64 count, int *fault);

// Trusted framework helpers; ranges must originate from a checked reservation.
void *klet_tio_pipe_address(struct klet_tio_pipe *pipe, u64 offset, u64 length);
int klet_tio_pipe_can_transfer(struct klet_tio_pipe *pipe, u64 owner, u32 role);
int klet_tio_pipe_transfer(struct klet_tio_pipe *pipe, u64 owner, u32 role, u64 target);

int klet_tio_pipe_notify(struct klet_tio_pipe *, u64 owner, u32 role, u32 operation, u32 value, struct klet_tio_binding *);
#endif
