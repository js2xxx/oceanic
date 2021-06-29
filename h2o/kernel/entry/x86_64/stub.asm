ExVec_DivideBy0      equ   0
ExVec_Debug          equ   1
ExVec_Nmi            equ   2
ExVec_Breakpoint     equ   3
ExVec_Overflow       equ   4
ExVec_Bound          equ   5
ExVec_InvalidOp      equ   6
ExVec_DeviceNa       equ   7
ExVec_DoubleFault    equ   8
ExVec_CoprocOverrun  equ   9
ExVec_InvalidTss     equ   10
ExVec_SegmentNa      equ   11
ExVec_StackFault     equ   12
ExVec_GeneralProt    equ   13
ExVec_PageFault      equ   14
ExVec_Spurious       equ   15
ExVec_FloatPoint     equ   16
ExVec_Alignment      equ   17
ExVec_MachineCheck   equ   18
ExVec_SimdExcep      equ   19
ExVec_Virtual        equ   20
ExVec_ControlProt    equ   21
ExVec_VmmComm        equ   29

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

; define_intr(vec, asm_name, name, err_vec)
%macro define_intr 4

global %2
extern %3
%2:
%if %4 != 0
      push  %4
%endif

      call  intr_entry

      mov   rdi, rsp
      call  %3

      jmp   intr_exit

%endmacro

[section .text]

; define_intr(vec, asm_name, name, has_code)
define_intr ExVec_DivideBy0,     rout_div_0,             hdl_div_0,              -1
define_intr ExVec_Overflow,      rout_overflow,          hdl_overflow,           -1
define_intr ExVec_CoprocOverrun, rout_coproc_overrun,    hdl_coproc_overrun,     -1
define_intr ExVec_InvalidTss,    rout_invalid_tss,       hdl_invalid_tss,        0
define_intr ExVec_SegmentNa,     rout_segment_na,        hdl_segment_na,         0
define_intr ExVec_StackFault,    rout_stack_fault,       hdl_stack_fault,        0

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

      swapgs
      jmp   .return
.reent:
      ; TODO: Handle some errors inside this routine.
.return:
      iretq