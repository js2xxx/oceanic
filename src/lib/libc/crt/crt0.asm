global _start:function

extern __libc_start_main
extern main

_start:
    lea rsi, [rel main]
    jmp __libc_start_main
