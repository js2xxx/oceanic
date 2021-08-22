extern kmain
extern INIT_STACK

[section .text]
global kentry
kentry:
      mov   rsp, INIT_STACK
      call  kmain

.lp:
      sti
      hlt
      jmp   .lp

global reset_seg
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
