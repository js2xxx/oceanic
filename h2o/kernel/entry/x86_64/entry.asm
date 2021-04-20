extern kmain
extern INIT_STACK

global kentry
global reset_seg

[section .text]
kentry:
      mov   rsp, INIT_STACK
      call  kmain

.lp:
      hlt
      jmp   .lp

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