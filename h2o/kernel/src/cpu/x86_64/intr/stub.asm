.equ KRL_CODE_X64, 0x08
.equ KRL_DATA_X64, 0x10
.equ USR_CODE_X86, 0x18
.equ USR_DATA_X64, 0x20
.equ USR_CODE_X64, 0x28 + 3

.equ GS_BASE,        0xc0000101
.equ KERNEL_GS_BASE, 0xc0000102

# xs_base + 15GPRs + errc + rip
.equ FRAME_CS, ((2+15+1+1)*8)

.equ KERNEL_GS_TSS_RSP0,            0
.equ KERNEL_GS_SYSCALL_USER_STACK,  8
.equ KERNEL_GS_SYSCALL_STACK,       16
.equ KERNEL_GS_KERNEL_FS,           24

.macro align_rsp scratch_reg
      mov   \scratch_reg, 0
      test  rsp, 0xF
      jnz   1f
      mov   \scratch_reg, 1
      sub   rsp, 8
1:
.endm

.macro recover_rsp scratch_reg
      cmp   \scratch_reg, 0
      je    1f
      add   rsp, 8
1:
.endm

.macro align_call func, scratch_reg
      align_rsp   scratch_reg=\scratch_reg
      call        \func
      recover_rsp scratch_reg=\scratch_reg
.endm

.macro push_xs msr
      push  rcx
      mov   rcx, \msr
      rdmsr
      shl   rdx, 32
      add   rax, rdx
      mov   rcx, [rsp]
      mov   [rsp], rax
.endm

.macro pop_xs msr
      mov   rax, [rsp]
      mov   [rsp], rcx
      mov   rdx, rax
      shr   rdx, 32
      mov   rcx, \msr
      wrmsr
      pop   rcx
.endm

# push_regs(bool save_ret_addr, bool gs_swapped)
.macro push_regs save_ret_addr, gs_swapped
.if \save_ret_addr
      push  rcx               # | ret_addr  |<-rsp | ret_addr  |      |    rax    |
      mov   rcx, [rsp + 8]    # |-----------| ---> |-----------| ---> |-----------|, rcx=ret_addr
      mov   [rsp + 8], rax    # |           |      |    rcx    |<-rsp |    rcx    |<-rsp
.else
      push  rax
      push  rcx
.endif
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
.if \gs_swapped
      push_xs KERNEL_GS_BASE
.else
      push  rcx
      rdgsbase  rcx
      xchg  [rsp], rcx
.endif
.if \save_ret_addr
      push  rcx
.endif

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
.endm

# pop_regs(bool gs_swapped)
.macro pop_regs gs_swapped
      cmp   qword ptr [rsp + FRAME_CS], 0x8 # KRL_CODE_X64
      jne   1f
      mov   ax, KRL_DATA_X64
      jmp   2f
1:
      mov   ax, USR_DATA_X64
2:
      mov   fs, ax
.if \gs_swapped
      pop_xs KERNEL_GS_BASE
.else
      xchg  rcx, [rsp]
      wrgsbase  rcx
      pop   rcx
.endif
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
.endm

# ---------------------------------------------------------------------------------------
# Interrupts

.equ ExVec_DivideBy0,     0
.equ ExVec_Debug,         1
.equ ExVec_Nmi,           2
.equ ExVec_Breakpoint,    3
.equ ExVec_Overflow,      4
.equ ExVec_Bound,         5
.equ ExVec_InvalidOp,     6
.equ ExVec_DeviceNa,      7
.equ ExVec_DoubleFault,   8
.equ ExVec_CoprocOverrun, 9
.equ ExVec_InvalidTss,    10
.equ ExVec_SegmentNa,     11
.equ ExVec_StackFault,    12
.equ ExVec_GeneralProt,   13
.equ ExVec_PageFault,     14
.equ ExVec_FloatPoint,    16
.equ ExVec_Alignment,     17
.equ ExVec_MachineCheck,  18
.equ ExVec_SimdExcep,     19
.equ ExVec_Virtual,       20
.equ ExVec_ControlProt,   21
.equ ExVec_VmmComm,       29

.equ ApicVec_Timer,          0x20
.equ ApicVec_Error,          0x21
.equ ApicVec_IpiTaskMigrate, 0x22
.equ ApicVec_Spurious,       0xFF

# define_intr(vec, asm_name, name, err_vec)
.macro define_intr vec, asm_name, name, err_vec=0

.global \asm_name
.type \asm_name, @function
.extern \name
\asm_name:
      .cfi_startproc
.if \err_vec != 0
      push  \err_vec
.endif

      call  intr_entry

      mov   rdi, rsp

      align_call  \name, r12

      jmp   intr_exit
      .cfi_endproc

.endm

.section .text

.global switch_kframe
.type switch_kframe, @function
# Switch the kernel frame of the current thread.
;
# # Safety
;
# The layout in the stack must matches [`h2o::sched::task::ctx::x86_64::Kframe`].
;
# switch_kframe(rdi *old_kframe, rsi new_kframe)
switch_kframe:
      .cfi_startproc
      push  rbp
      push  rbx
      push  r12
      push  r13
      push  r14
      push  r15
      pushfq
      push  qword ptr [rdx]
      push  qword ptr [rcx]
      xor   rax, rax
      mov   ax, cs
      push  rax

      # Save the current stack context.
      cmp   rdi, 0
      je    1f
      mov   [rdi], rsp
1:
      # Switch the stack (a.k.a. the context).
      mov   rsp, rsi

      lea   rdi, [rip + 2f]
      push  rdi
      retfq
2:
      pop   qword ptr [rcx]
      pop   qword ptr [rdx]
      popfq
      pop   r15
      pop   r14
      pop   r13
      pop   r12
      pop   rbx
      pop   rbp
      ret
      .cfi_endproc

.global task_fresh
.type task_fresh, @function
.extern switch_finishing
# The entry into the interrupt context of a new task.
task_fresh:
      .cfi_startproc
      xor   rdi, rdi
      xor   rsi, rsi
      align_call switch_finishing, r12
      cli
      jmp   intr_exit
      .cfi_endproc

# define_intr(vec, asm_name, name, has_code)

# x86 exceptions
define_intr vec=ExVec_DivideBy0,     asm_name=rout_div_0,          name=hdl_div_0,          err_vec=-1
define_intr vec=ExVec_Debug,         asm_name=rout_debug,          name=hdl_debug,          err_vec=-1
define_intr vec=ExVec_Nmi,           asm_name=rout_nmi,            name=hdl_nmi,            err_vec=-1
define_intr vec=ExVec_Breakpoint,    asm_name=rout_breakpoint,     name=hdl_breakpoint,     err_vec=-1
define_intr vec=ExVec_Overflow,      asm_name=rout_overflow,       name=hdl_overflow,       err_vec=-1
define_intr vec=ExVec_Bound,         asm_name=rout_bound,          name=hdl_bound,          err_vec=-1
define_intr vec=ExVec_InvalidOp,     asm_name=rout_invalid_op,     name=hdl_invalid_op,     err_vec=-1
define_intr vec=ExVec_DeviceNa,      asm_name=rout_device_na,      name=hdl_device_na,      err_vec=-1
define_intr vec=ExVec_DoubleFault,   asm_name=rout_double_fault,   name=hdl_double_fault,   err_vec=0
define_intr vec=ExVec_CoprocOverrun, asm_name=rout_coproc_overrun, name=hdl_coproc_overrun, err_vec=-1
define_intr vec=ExVec_InvalidTss,    asm_name=rout_invalid_tss,    name=hdl_invalid_tss,    err_vec=0
define_intr vec=ExVec_SegmentNa,     asm_name=rout_segment_na,     name=hdl_segment_na,     err_vec=0
define_intr vec=ExVec_StackFault,    asm_name=rout_stack_fault,    name=hdl_stack_fault,    err_vec=0
define_intr vec=ExVec_GeneralProt,   asm_name=rout_general_prot,   name=hdl_general_prot,   err_vec=0
define_intr vec=ExVec_PageFault,     asm_name=rout_page_fault,     name=hdl_page_fault,     err_vec=0
define_intr vec=ExVec_FloatPoint,    asm_name=rout_fp_excep,       name=hdl_fp_excep,       err_vec=-1
define_intr vec=ExVec_Alignment,     asm_name=rout_alignment,      name=hdl_alignment,      err_vec=-1
# define_intr vec=ExVec_MachineCheck,     asm_name=rout_mach_check,        hdl_mach_check,         0
define_intr vec=ExVec_SimdExcep,     asm_name=rout_simd,           name=hdl_simd,           err_vec=0

# Local APIC interrupts
define_intr vec=ApicVec_Timer,          asm_name=rout_lapic_timer,             name=hdl_lapic_timer,              err_vec=-1
define_intr vec=ApicVec_Error,          asm_name=rout_lapic_error,             name=hdl_lapic_error,              err_vec=-1
define_intr vec=ApicVec_IpiTaskMigrate, asm_name=rout_lapic_ipi_task_migrate,  name=hdl_lapic_ipi_task_migrate,   err_vec=-1
define_intr vec=ApicVec_Spurious,       asm_name=rout_lapic_spurious,          name=hdl_lapic_spurious,           err_vec=-1

# All other interrupts
define_intr -1, rout_common_interrupt, common_interrupt
.global common_intr_entries
.align 8
common_intr_entries:
.set i, 0x40
.rept (0xFF - 0x40)
1:
      .byte 0x6a, i # push i
      jmp rout_common_interrupt
      .byte 0xcc
      .set i, (i + 1)
.endr

.extern save_regs# Save the GPRs from the current stack and switch to the task's `intr_stack`.

.type intr_entry, @function
intr_entry:
      .cfi_startproc
      cld

      cmp   qword ptr [rsp + 8 * 3], 0xc# Test if it's a reentrancy.
      je    2f

      swapgs
      lfence

      # If we came from a kernel task, manually switch to its kernel stack.
      cmp   qword ptr [rsp + 8 * 3], 0x8
      jne   1f

      push  rdi
      mov   rdi, rsp
      mov   rsp, gs:[KERNEL_GS_TSS_RSP0]
      push  qword ptr [rdi + 8 * 7]
      push  qword ptr [rdi + 8 * 6]
      push  qword ptr [rdi + 8 * 5]
      push  qword ptr [rdi + 8 * 4]
      push  qword ptr [rdi + 8 * 3]
      push  qword ptr [rdi + 8 * 2]
      push  qword ptr [rdi + 8]
      mov   rdi, [rdi]

1:
      push_regs   1, 1# The routine has a return address, so we must preserve it.
      lea   rbp, [rsp + 8 + 1]

      mov   rax, gs:[(KERNEL_GS_KERNEL_FS)]
      wrfsbase rax

      align_call  save_regs, r12

      jmp   3f
2:
      # A preemption happens.
      lfence

      push_regs   1, 0# The routine has a return address, so we must preserve it.
      lea   rbp, [rsp + 8 + 1]
3:
      ret
      .cfi_endproc

intr_exit:
      .cfi_startproc
      cmp   qword ptr [rsp + FRAME_CS], 0xc# Test if it's a reentrancy.
      je    4f

      pop_regs    1

      # The stack now consists of errc and 'iretq stuff'
      add   rsp, 8

      swapgs
      jmp   5f
4:
      # A preemption happens.
      pop_regs    0

      add   rsp, 8
5:
      iretq
      .cfi_endproc

# ---------------------------------------------------------------------------------------
# Syscalls

.global rout_syscall
.type rout_syscall, function
.extern hdl_syscall
rout_syscall:
      .cfi_startproc
      swapgs

      mov   gs:[(KERNEL_GS_SYSCALL_USER_STACK)], rsp
      mov   rsp, gs:[(KERNEL_GS_TSS_RSP0)]

      # Fit the context to [`x86_64::Frame`]
      push  qword ptr USR_DATA_X64                         # ss
      push  qword ptr gs:[(KERNEL_GS_SYSCALL_USER_STACK)]  # rsp
      push  r11                                            # rflags
      push  qword ptr USR_CODE_X64                         # cs
      push  rcx                                            # rip
      push  -1                                             # errc_vec

      push_regs   0, 1
      lea   rbp, [rsp + 8 + 1]

      mov   rax, gs:[(KERNEL_GS_KERNEL_FS)]
      wrfsbase rax

      mov   rcx, GS_BASE
      rdmsr
      mov   rcx, KERNEL_GS_BASE
      wrmsr

      align_call  save_regs, r12

      mov   rdi, rsp

      align_call  hdl_syscall, r12

      pop_regs    1
      add   rsp, 8            # errc_vec

      # Here we must test the return address because Intel's mistake of `sysret`
      # machanism. Normally, only codes on ring 3 (lower half) can execute `syscall`.
      # See https://xenproject.org/2012/06/13/the-intel-sysret-privilege-escalation/
      test  dword ptr [rsp + 4], 0xFFFF8000 # test if the return address is on the higher half
      jnz   1f

      cmp   qword ptr [rsp + 8], 0x8 # test if the return segment is kernel.
      je    1f

      pop   rcx                                       # rip
      add   rsp, 8                                    # cs
      pop   r11                                       # rflags
      # pop  qword ptr gs:[(KERNEL_GS_SYSCALL_USER_STACK)] # rsp
      # ;add rsp, 8                                   # ss
      # mov  rsp, gs:[(KERNEL_GS_SYSCALL_USER_STACK)]
      pop   rsp   # simplify 3 instructions above

      swapgs
      sysretq

1:
      xor   rcx, rcx
      xor   r11, r11

      swapgs
      iretq
      .cfi_endproc
