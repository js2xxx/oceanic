.extern kmain
.extern INIT_STACK

.section .text.init
.global kentry
.type kentry, @function
kentry:
    .cfi_startproc

    lea   rsp, [rip + INIT_STACK]
    call  kmain

.lp:
    sti
    hlt
    jmp   .lp

    .cfi_endproc

.section .text

.global reset_seg
.type reset_seg, @function

# fn reset_seg(di code_selector, si data_selector)
reset_seg:
    .cfi_startproc

    push  rdi
    lea   rdi, [rip + .Lret_rs]
    push  rdi
    retfq

.Lret_rs:
    mov   ds, si
    mov   es, si
    mov   ss, si
    
    ret
    .cfi_endproc

.global cpu_in_intr
.type cpu_in_intr, @function

# fn cpu_in_intr() -> u32
cpu_in_intr:
    .cfi_startproc

    mov   ax, cs
    cmp   ax, 0xC
    je    .Ltrue
    mov   eax, 0
    jmp   .Lret
.Ltrue:
    mov   eax, 1
.Lret:
    ret
    .cfi_endproc

.global checked_copy
.type checked_copy, @function
# fn checked_copy(
#   (rdi) dst: *mut u64, 
#   (rsi) src: *const u64, 
#   (rdx) pf_resume: *mut Option<NonZeroU64>,
#   (rcx) count: usize, 
# ) -> (usize, usize)
checked_copy:
    .cfi_startproc

    lea   rax, [rip + .Lfault]
    mov   r11, rdx
    mov   [r11], rax

    mov   r10, rcx
    shr   rcx, 3
    rep   movsq
    and   r10, 7
    jz    .Lok
    mov   rcx, r10
    rep   movsb
.Lok:
    xor   rdx, rdx
    xor   rax, rax

.Lret_copy:
    mov   qword ptr [r11], 0
    mov   r11, 0
    ret

.Lfault:
    add   rdx, 1
    jmp   .Lret_copy
    .cfi_endproc
