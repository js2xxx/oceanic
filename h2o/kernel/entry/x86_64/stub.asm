ExVec_DivideBy0         equ   0
ExVec_Debug             equ   1
ExVec_Nmi               equ   2
ExVec_Breakpoint        equ   3
ExVec_Overflow          equ   4
ExVec_Bound             equ   5
ExVec_InvalidOp         equ   6
ExVec_DeviceNa          equ   7
ExVec_DoubleFault       equ   8
ExVec_CoprocOverrun     equ   9
ExVec_InvalidTss        equ   10
ExVec_SegmentNa         equ   11
ExVec_StackFault        equ   12
ExVec_GeneralProt       equ   13
ExVec_PageFault         equ   14
ExVec_FloatPoint        equ   16
ExVec_Alignment         equ   17
ExVec_MachineCheck      equ   18
ExVec_SimdExcep         equ   19
ExVec_Virtual           equ   20
ExVec_ControlProt       equ   21
ExVec_VmmComm           equ   29

ApicVec_Timer     equ   0x20
ApicVec_Ipi       equ   0x21
ApicVec_Error     equ   0x22
ApicVec_Spurious  equ   0xFF

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

; x86 exceptions
define_intr ExVec_DivideBy0,        rout_div_0,             hdl_div_0,              -1
define_intr ExVec_Debug,            rout_debug,             hdl_debug,              -1
define_intr ExVec_Nmi,              rout_nmi,               hdl_nmi,                -1
define_intr ExVec_Breakpoint,       rout_breakpoint,        hdl_breakpoint,         -1
define_intr ExVec_Overflow,         rout_overflow,          hdl_overflow,           -1
define_intr ExVec_Bound,            rout_bound,             hdl_bound,              -1
define_intr ExVec_InvalidOp,        rout_invalid_op,        hdl_invalid_op,         -1
define_intr ExVec_DeviceNa,         rout_device_na,         hdl_device_na,          -1
define_intr ExVec_DoubleFault,      rout_double_fault,      hdl_double_fault,       0
define_intr ExVec_CoprocOverrun,    rout_coproc_overrun,    hdl_coproc_overrun,     -1
define_intr ExVec_InvalidTss,       rout_invalid_tss,       hdl_invalid_tss,        0
define_intr ExVec_SegmentNa,        rout_segment_na,        hdl_segment_na,         0
define_intr ExVec_StackFault,       rout_stack_fault,       hdl_stack_fault,        0
define_intr ExVec_GeneralProt,      rout_general_prot,      hdl_general_prot,       0
define_intr ExVec_PageFault,        rout_page_fault,        hdl_page_fault,         0
define_intr ExVec_FloatPoint,       rout_fp_excep,          hdl_fp_excep,           -1
define_intr ExVec_Alignment,        rout_alignment,         hdl_alignment,          -1
; define_intr ExVec_MachineCheck,     rout_mach_check,        hdl_mach_check,         0
define_intr ExVec_SimdExcep,        rout_simd,              hdl_simd,               0

; Local APIC interrupts
define_intr ApicVec_Timer,          rout_lapic_timer,       hdl_lapic_timer,        -1
define_intr ApicVec_Spurious,       rout_lapic_spurious,    hdl_lapic_spurious,     -1
define_intr ApicVec_Error,          rout_lapic_error,       hdl_lapic_error,        -1

; All other interrupts
%define rout_name(x) rout_ %+ x
%assign i 0x40
%rep (0xFF - 0x40)

define_intr i, rout_name(i), common_interrupt, i

%assign i (i + 1)
%endrep
%undef rout_name

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