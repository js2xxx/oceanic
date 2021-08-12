bits  16
      jmp   start

times 16 - ($-$$) db    0
      booted      dq    0
      stack       dq    0
      pgc         dq    0
      tls         dq    0
      
      kmain       dq    0
      init_efer   dq    0
      init_cr4    dq    0
      init_cr0    dq    0
      gdt   dq    0, 0, 0

code  equ   8
data  equ   16

start:
      cli

o32   lgdt  [.gdtr]

      mov   eax, 0b100000                 ; PAE
      mov   cr4, eax

      mov   eax, [pgc]
      mov   cr3, eax

      mov   eax, [init_efer]
      mov   edx, [init_efer + 4],
      mov   ecx, 0xc0000080
      wrmsr

      mov   eax, 0x80000001               ; PE | PG
      mov   cr0, eax

      jmp   dword code:.c64

bits 64
.c64:
      mov   si, data
      mov   ds, si
      mov   es, si
      mov   ss, si

      mov   rax, [init_cr0]
      mov   cr0, rax

      mov   rax, [init_cr4]
      mov   cr4, rax

      mov   rsp, [stack]

      mov   rax, [tls]
      mov   rdx, rax
      shr   rdx, 32
      mov   rcx, 0xc0000100
      wrmsr

lock  bts   qword [booted], 0

      mov   rax, [kmain]
      call  rax

.lp:
      hlt
      jmp   .lp

.gdtr       dw    3 * 8 - 1
            dd    gdt