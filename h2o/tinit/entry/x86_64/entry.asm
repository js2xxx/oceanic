[section .text]

global _start
_start:
.lp:
      ; mov   rax, 0x1234567890
      ; mov   dword [rax], 0
      pause
      jmp   .lp
