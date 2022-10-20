KRL_CODE_X64 equ 0x08
KRL_DATA_X64 equ 0x10
USR_CODE_X86 equ 0x18
USR_DATA_X64 equ 0x20
USR_CODE_X64 equ 0x28 + 3

GS_BASE           equ 0xc0000101
KERNEL_GS_BASE    equ 0xc0000102

struc Frame
      .gs_base resq 1
      .fs_base resq 1

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
      .tss_rsp0               resq 1
      .syscall_user_stack     resq 1
      .syscall_stack          resq 1
      .kernel_fs              resq 1
endstruc

%macro align_rsp 1
      mov   %1, 0
      test  rsp, 0xF
      jnz   %%next
      mov   %1, 1
      sub   rsp, 8
%%next:
%endmacro

%macro recover_rsp 1
      cmp   %1, 0
      je    %%next
      add   rsp, 8
%%next:
%endmacro

%macro align_call 2
      align_rsp   %2
      call        %1
      recover_rsp %2
%endmacro

%macro push_xs 1
      push  rcx
      mov   rcx, %1
      rdmsr
      shl   rdx, 32
      add   rax, rdx
      mov   rcx, [rsp]
      mov   [rsp], rax
%endmacro

%macro pop_xs 1
      mov   rax, [rsp]
      mov   [rsp], rcx
      mov   rdx, rax
      shr   rdx, 32
      mov   rcx, %1
      wrmsr
      pop   rcx
%endmacro

; push_regs(bool save_ret_addr, bool gs_swapped)
%macro push_regs 2
%if %1 == 1
      push  rcx               ; | ret_addr  |<-rsp | ret_addr  |      |    rax    |
      mov   rcx, [rsp + 8]    ; |-----------| ---> |-----------| ---> |-----------|, rcx=ret_addr
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

      push  rcx
      rdfsbase  rcx
      xchg  [rsp], rcx
%if %2 == 1
      push_xs KERNEL_GS_BASE
%else
      push  rcx
      rdgsbase  rcx
      xchg  [rsp], rcx
%endif
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

; pop_regs(bool gs_swapped)
%macro pop_regs 1
      cmp   word [rsp + Frame.cs], KRL_CODE_X64
      jne   %%set_user
      mov   ax, KRL_DATA_X64
      jmp   %%pop
%%set_user:
      mov   ax, USR_DATA_X64
%%pop:
      mov   fs, ax
%if %1 == 1
      pop_xs KERNEL_GS_BASE
%else
      xchg  rcx, [rsp]
      wrgsbase  rcx
      pop   rcx
%endif
      xchg  rcx, [rsp]
      wrfsbase  rcx
      pop   rcx

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

; ---------------------------------------------------------------------------------------
; Interrupts

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

ApicVec_Timer           equ   0x20
ApicVec_Error           equ   0x21
ApicVec_IpiTaskMigrate  equ   0x22
ApicVec_Spurious        equ   0xFF

; define_intr(vec, asm_name, name, err_vec)
%macro define_intr 4

global %2:function
extern %3
%2:
%if %4 != 0
      push  %4
%endif

      call  intr_entry

      mov   rdi, rsp

      align_call  %3, r12

      jmp   intr_exit

%endmacro

[section .text]

global switch_kframe:function
; Switch the kernel frame of the current thread.
;
; # Safety
;
; The layout in the stack must matches [`h2o::sched::task::ctx::x86_64::Kframe`].
;
; switch_kframe(rdi *old_kframe, rsi new_kframe)
switch_kframe:
      push  rbp
      push  rbx
      push  r12
      push  r13
      push  r14
      push  r15
      pushfq
      push  qword [rdx]
      push  qword [rcx]
      xor   rax, rax
      mov   ax, cs
      push  rax

      ; Save the current stack context.
      cmp   rdi, 0
      je    .switch
      mov   [rdi], rsp
.switch:
      ; Switch the stack (a.k.a. the context).
      mov   rsp, rsi

      push  .pop_regs
      retfq
.pop_regs:
      pop   qword [rcx]
      pop   qword [rdx]
      popfq
      pop   r15
      pop   r14
      pop   r13
      pop   r12
      pop   rbx
      pop   rbp
      ret

global task_fresh:function
extern switch_finishing
; The entry into the interrupt context of a new task.
task_fresh:
      xor   rdi, rdi
      xor   rsi, rsi
      align_call switch_finishing, r12
      cli
      jmp   intr_exit

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
define_intr ApicVec_Timer,          rout_lapic_timer,             hdl_lapic_timer,              -1
define_intr ApicVec_Error,          rout_lapic_error,             hdl_lapic_error,              -1
define_intr ApicVec_IpiTaskMigrate, rout_lapic_ipi_task_migrate,  hdl_lapic_ipi_task_migrate,   -1
define_intr ApicVec_Spurious,       rout_lapic_spurious,          hdl_lapic_spurious,           -1

; All other interrupts
%define rout_name(x) rout_ %+ x
%assign i 0x40
%rep (0xFF - 0x40)

define_intr i, rout_name(i), common_interrupt, i

%assign i (i + 1)
%endrep
%undef rout_name

extern save_regs; Save the GPRs from the current stack and switch to the task's `intr_stack`.

intr_entry:
      cld

      cmp   qword [rsp + 8 * 3], 0xc; Test if it's a reentrancy.
      je    .reent

      swapgs
      lfence

      ; If we came from a kernel task, manually switch to its kernel stack.
      cmp   qword [rsp + 8 * 3], 0x8
      jne   .push_regs

      push  rdi
      mov   rdi, rsp
      mov   rsp, [gs:KernelGs.tss_rsp0]
      push  qword [rdi + 8 * 7]
      push  qword [rdi + 8 * 6]
      push  qword [rdi + 8 * 5]
      push  qword [rdi + 8 * 4]
      push  qword [rdi + 8 * 3]
      push  qword [rdi + 8 * 2]
      push  qword [rdi + 8]
      mov   rdi, [rdi]

.push_regs:
      push_regs   1, 1; The routine has a return address, so we must preserve it.
      lea   rbp, [rsp + 8 + 1]

      mov   rax, [gs:(KernelGs.kernel_fs)]
      wrfsbase rax

      align_call  save_regs, r12

      jmp   .ret
.reent:
      ; A preemption happens.
      lfence

      push_regs   1, 0; The routine has a return address, so we must preserve it.
      lea   rbp, [rsp + 8 + 1]
.ret:
      ret

intr_exit:
      cmp   qword [rsp + Frame.cs], 0xc; Test if it's a reentrancy.
      je    .reent

      pop_regs    1

      ; The stack now consists of errc and 'iretq stuff'
      add   rsp, 8

      swapgs
      jmp   .ret
.reent:
      ; A preemption happens.
      pop_regs    0

      add   rsp, 8
.ret:
      iretq

; ---------------------------------------------------------------------------------------
; Syscalls

global rout_syscall:function
extern hdl_syscall
rout_syscall:
      swapgs

      mov   [gs:(KernelGs.syscall_user_stack)], rsp
      mov   rsp, [gs:(KernelGs.tss_rsp0)]

      ; Fit the context to [`x86_64::Frame`]
      push  qword USR_DATA_X64                        ; ss
      push  qword [gs:(KernelGs.syscall_user_stack)]  ; rsp
      push  r11                                       ; rflags
      push  qword USR_CODE_X64                        ; cs
      push  rcx                                       ; rip
      push  -1                                        ; errc_vec

      push_regs   0, 1
      lea   rbp, [rsp + 8 + 1]

      mov   rax, [gs:(KernelGs.kernel_fs)]
      wrfsbase rax

      mov   rcx, GS_BASE
      rdmsr
      mov   rcx, KERNEL_GS_BASE
      wrmsr

      align_call  save_regs, r12

      mov   rdi, rsp

      align_call  hdl_syscall, r12

      pop_regs    1
      add   rsp, 8            ; errc_vec

      ; Here we must test the return address because Intel's mistake of `sysret`
      ; machanism. Normally, only codes on ring 3 (lower half) can execute `syscall`.
      ; See https://xenproject.org/2012/06/13/the-intel-sysret-privilege-escalation/
      test  dword [rsp + 4], 0xFFFF8000 ; test if the return address is on the higher half
      jnz   .fault_iret

      cmp   qword [rsp + 8], 0x8 ; test if the return segment is kernel.
      je    .fault_iret

      pop   rcx                                       ; rip
      add   rsp, 8                                    ; cs
      pop   r11                                       ; rflags
      ; pop  qword [gs:(KernelGs.syscall_user_stack)] ; rsp
      ; ;add rsp, 8                                   ; ss
      ; mov  rsp, [gs:(KernelGs.syscall_user_stack)]
      pop   rsp   ; simplify 3 instructions above

      swapgs
o64   sysret

.fault_iret:
      xor   rcx, rcx
      xor   r11, r11

      swapgs
      iretq
