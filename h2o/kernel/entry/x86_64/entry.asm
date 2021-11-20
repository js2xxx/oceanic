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
