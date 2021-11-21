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
; void reset_seg(di code_selector, si data_selector)
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
; u32 cpu_in_intr()
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