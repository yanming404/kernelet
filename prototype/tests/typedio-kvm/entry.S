// SPDX-License-Identifier: MPL-2.0
.text
.global _start
_start:
    xor %ebp, %ebp
    mov (%rsp), %rdi
    lea 8(%rsp), %rsi
    and $-16, %rsp
    call experiment_main
1:
    mov $34, %eax
    syscall
    jmp 1b

// Clone a thread using a fresh aligned stack without returning through C.
.global experiment_thread
.type experiment_thread,@function
experiment_thread:
    push %r12
    push %r13
    mov %rdi, %r12
    mov %rdx, %r13
    mov $0x50f00, %edi
    xor %edx, %edx
    xor %r10d, %r10d
    xor %r8d, %r8d
    mov $56, %eax
    syscall
    test %rax, %rax
    jnz 2f
    mov %r13, %rdi
    call *%r12
    xor %edi, %edi
    mov $60, %eax
    syscall
    ud2
2:
    pop %r13
    pop %r12
    ret
.size experiment_thread,.-experiment_thread

.global experiment_sigreturn
.type experiment_sigreturn,@function
experiment_sigreturn:
    mov $15, %eax
    syscall
    ud2
.size experiment_sigreturn,.-experiment_sigreturn

.global platform_set_xmm
.type platform_set_xmm,@function
platform_set_xmm:
    movdqu (%rdi), %xmm8
    ret
.size platform_set_xmm,.-platform_set_xmm
.global platform_get_xmm
.type platform_get_xmm,@function
platform_get_xmm:
    movdqu %xmm8, (%rdi)
    ret
.size platform_get_xmm,.-platform_get_xmm
.section .note.GNU-stack,"",@progbits
