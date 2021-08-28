extern tmain
[section .text]

global _start
_start:
      call  tmain
.lp:
      ; mov   dword [rax], 0
      pause
      jmp   .lp
