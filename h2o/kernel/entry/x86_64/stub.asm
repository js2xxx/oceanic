struc Frame
      .r15  resq 1
      .r14  resq 1
      .r13  resq 1
      .r12  resq 1
      .r11  resq 1
      .r10  resq 1
      .r9   resq 1
      .r8   resq 1
      .rsi  resq 1
      .rdi  resq 1
      .rbp  resq 1
      .rbx  resq 1
      .rdx  resq 1
      .rcx  resq 1
      .rax  resq 1

      .errc resq 1

      .rip  resq 1
      .cs   resq 1
      .rflags resq 1
      .rsp  resq 1
      .ss   resq 1
endstruc

struc KernelGs
      .save_regs        resq 1 ; Save the GPRs from the current stack and *return the thread stack*
      .tss_rsp0         resq 1
endstruc

; push_regs(bool save_ret_addr)
%macro push_regs 1
%if %1 == 1
      push  rcx               ; | ret_addr  |<-rsp | ret_addr  |      |    rax    |
      mov   rcx, [rsp + 8]    ; |-----------| ---> |-----------| ---> |-----------|, rax=ret_addr
      mov   [rsp + 8], rax    ; |           |      |    rcx    |<-rsp |    rcx    |<-rsp
%else
      push  rax
      push  rcx
%endif
      push  rdx
      push  rbx
      push  rbp
      push  rdi
      push  rsi
      push  r8
      push  r9
      push  r10
      push  r11
      push  r12
      push  r13
      push  r14
      push  r15
%if %1 == 1
      push  rcx
%endif

      xor   rcx, rcx
      xor   rdx, rdx
      xor   rbx, rbx
      xor   rbp, rbp
      xor   r8, r8
      xor   r9, r9
      xor   r10, r10
      xor   r11, r11
      xor   r12, r12
      xor   r13, r13
      xor   r14, r14
      xor   r15, r15
%endmacro

%macro pop_regs 0
      pop   r15
      pop   r14
      pop   r13
      pop   r12
      pop   r11
      pop   r10
      pop   r9
      pop   r8
      pop   rsi
      pop   rdi
      pop   rbp
      pop   rbx
      pop   rdx
      pop   rcx
      pop   rax
%endmacro

; define_intr(vec, asm_name, name, has_code)
%macro define_intr 4

global %2
extern %3
%2:
%if %4 == 1
      push  -1
%endif

      call  intr_entry

      mov   rdi, rsp
      call  %3

      jmp   intr_exit

%endmacro

[section .text]

; define_intr 1, rout_dummy, hdl_dummy, 0

intr_entry:
      cld
      push_regs   1; The routine has a return address, so we must preseve it.
      lea   rbp, [rsp + 8 + 1]

      bt    qword [rsp + (8 + Frame.cs)], 2; Test if it's a reentrancy.
      jc    .reent

      swapgs
      lfence

      pop   r12
      mov   rdi, rsp
      mov   rax, [gs:(KernelGs.save_regs)]
      call  rax
      mov   rsp, rax
      lea   rbp, [rsp + 1]
      push  r12

      ret
.reent:
      ; TODO: Handle some errors inside this routine.
      ret

intr_exit:
      bt    qword [rsp + Frame.cs], 2; Test if it's a reentrancy.
      jc    .reent

      pop_regs

      ; The stack now consists of errc and 'iretq stuff'
      push  rdi
      mov   rdi, rsp
      mov   rsp, [gs:(KernelGs.tss_rsp0)]
      push  qword [rdi + 8 * 6]     ; ss
      push  qword [rdi + 8 * 5]     ; rsp
      push  qword [rdi + 8 * 4]     ; rflags
      push  qword [rdi + 8 * 3]     ; cs
      push  qword [rdi + 8 * 2]     ; rip
      mov   rdi, [rdi]

      jmp   .return
.reent:
      ; TODO: Handle some errors inside this routine.
.return:
      iretq