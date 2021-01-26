extern kmain

global kentry

[section .text]
kentry:
      call  kmain

.lp:
      hlt
      jmp   .lp