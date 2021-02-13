extern kmain
extern INIT_STACK

global kentry

[section .text]
kentry:
      mov   rsp, INIT_STACK
      call  kmain

.lp:
      hlt
      jmp   .lp