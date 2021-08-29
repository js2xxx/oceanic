extern tmain
[section .text]

global _start
_start:
      sub   rsp, 8
      call  tmain
.lp:
      ; mov   dword [rax], 0
      pause
      jmp   .lp
