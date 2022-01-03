extern kmain
extern INIT_STACK

[section .text]
global kentry:function
kentry:
      mov   rsp, INIT_STACK
      call  kmain

.lp:
      sti
      hlt
      jmp   .lp

global reset_seg:function
; fn reset_seg(di code_selector, si data_selector)
reset_seg:
      push  rdi
      push  .ret
      retfq

.ret:
      mov   ds, si
      mov   es, si
      mov   ss, si
      
      ret

global cpu_in_intr:function
; fn cpu_in_intr() -> u32
cpu_in_intr:
      mov   ax, cs
      cmp   ax, 0xC
      je    .true
      mov   eax, 0
      jmp   .ret
.true:
      mov   eax, 1
.ret:
      ret

global checked_copy:function
; fn checked_copy(
; (rdi) dst: *mut u64, 
; (rsi) src: *const u64, 
; (rdx) pf_resume: *mut Option<NonZeroU64>,
; (rcx) count: usize, 
;) -> (usize, usize)
checked_copy:
      mov   rax, .fault
      mov   r11, rdx
      mov   [r11], rax

      mov   r10, rcx
      shr   rcx, 3
      rep   movsq
      and   r10, 7
      jz    .ok
      mov   rcx, r10
      rep   movsb
.ok:
      xor   rdx, rdx
      xor   rax, rax

.ret:
      mov   qword [r11], 0
      mov   r11, 0
      ret

.fault:
      add   rdx, 1
      jmp   .ret
