use alloc::sync::Arc;
use core::{alloc::Layout, mem::size_of};

use paging::LAddr;
use sv_call::call::Syscall;

use super::Entry;
use crate::{
    cpu::{
        self,
        arch::{
            seg::{
                ndt::{KRL_CODE_X64, KRL_DATA_X64, USR_CODE_X64, USR_DATA_X64},
                SegSelector,
            },
            KERNEL_GS,
        },
    },
    sched::{task, PREEMPT},
};

pub const DEFAULT_STACK_SIZE: usize = 64 * paging::PAGE_SIZE;
pub const DEFAULT_STACK_LAYOUT: Layout =
    unsafe { Layout::from_size_align_unchecked(DEFAULT_STACK_SIZE, paging::PAGE_SIZE) };

pub const EXTENDED_FRAME_SIZE: usize = 576;

#[derive(Debug, Default)]
#[repr(C)]
pub struct Kframe {
    cs: u64,
    rflags: u64,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
    ret_addr: u64,
}

impl Kframe {
    pub fn new(ptr: *const u8, cs: u64) -> Self {
        Kframe {
            cs,
            ret_addr: task_fresh as usize as u64,
            rbp: ptr as u64 + 1,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Frame {
    gs_base: u64,
    fs_base: u64,

    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rbx: u64,
    rdx: u64,
    rcx: u64,
    rax: u64,

    pub errc_vec: u64,

    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl Frame {
    #[inline]
    pub fn init_zeroed(&mut self, ty: task::Type) {
        let (cs, ss) = match ty {
            task::Type::User => (USR_CODE_X64, USR_DATA_X64),
            task::Type::Kernel => (KRL_CODE_X64, KRL_DATA_X64),
        };

        self.cs = SegSelector::into_val(cs) as u64;
        self.ss = SegSelector::into_val(ss) as u64;
    }

    pub fn init_entry(&mut self, entry: &Entry, ty: task::Type) {
        self.init_zeroed(ty);

        self.rip = entry.entry.val() as u64;
        self.rsp = (entry.stack.val() - size_of::<usize>()) as u64;
        self.rflags = archop::reg::rflags::IF;

        if matches!(ty, task::Type::Kernel) {
            self.gs_base = unsafe { crate::cpu::arch::KERNEL_GS.as_ptr() } as u64;
        }

        self.rdi = entry.args[0];
        self.rsi = entry.args[1];
    }

    #[inline]
    pub fn syscall_args(&self) -> Syscall {
        Syscall {
            num: self.rax as usize,
            args: [
                self.rdi as usize,
                self.rsi as usize,
                self.rdx as usize,
                self.r8 as usize,
                self.r9 as usize,
            ],
            ..Default::default()
        }
    }

    #[inline]
    pub fn set_syscall_retval(&mut self, syscall: &Syscall) {
        self.rax = syscall.result as u64;
    }

    #[inline]
    pub fn set_pf_resume(&mut self, rip: u64, errc: u64, addr: u64) {
        self.rip = rip;
        self.rax = errc;
        self.rdx = addr;
    }

    #[inline]
    pub fn debug_get(&self) -> sv_call::task::ctx::Gpr {
        sv_call::task::ctx::Gpr {
            rax: self.rax,
            rcx: self.rcx,
            rdx: self.rdx,
            rbx: self.rbx,
            rbp: self.rbp,
            rsp: self.rsp,
            rsi: self.rsi,
            rdi: self.rdi,
            r8: self.r8,
            r9: self.r9,
            r10: self.r10,
            r11: self.r11,
            r12: self.r12,
            r13: self.r13,
            r14: self.r14,
            r15: self.r15,
            rip: self.rip,
            rflags: self.rflags,
            fs_base: self.fs_base,
            gs_base: self.gs_base,
        }
    }

    /// # Errors
    ///
    /// Returns error if `fs_base` or `gs_base` is invalid.
    #[inline]
    pub fn debug_set(&mut self, gpr: &sv_call::task::ctx::Gpr) -> sv_call::Result<()> {
        if !archop::canonical(LAddr::from(gpr.fs_base))
            || !archop::canonical(LAddr::from(gpr.gs_base))
        {
            return Err(sv_call::EINVAL);
        }
        self.gs_base = gpr.gs_base;
        self.fs_base = gpr.fs_base;

        self.r15 = gpr.r15;
        self.r14 = gpr.r14;
        self.r13 = gpr.r13;
        self.r12 = gpr.r12;
        self.r11 = gpr.r11;
        self.r10 = gpr.r10;
        self.r9 = gpr.r9;
        self.r8 = gpr.r8;
        self.rsi = gpr.rsi;
        self.rdi = gpr.rdi;
        self.rbp = gpr.rbp;
        self.rbx = gpr.rbx;
        self.rdx = gpr.rdx;
        self.rcx = gpr.rcx;
        self.rax = gpr.rax;
        self.rip = gpr.rip;
        self.rsp = gpr.rsp;

        self.rflags &= !archop::reg::rflags::USER_ACCESS;
        self.rflags |= gpr.rflags & archop::reg::rflags::USER_ACCESS;

        Ok(())
    }

    const RFLAGS: &'static str =
        "CF - PF - AF - ZF SF TF IF DF OF IOPLL IOPLH NT - RF VM AC VIF VIP ID";

    pub const ERRC: &'static str = "EXT IDT TI";
    pub const ERRC_PF: &'static str = "P WR US RSVD ID PK SS - - - - - - - - SGX";

    pub fn dump(&self, errc_format: &'static str) {
        use log::info;

        use crate::log::flags::Flags;

        info!("Frame dump on CPU #{}", unsafe { crate::cpu::id() });

        if self.errc_vec != 0u64.wrapping_sub(1) && !errc_format.is_empty() {
            info!("> Error Code = {}", Flags::new(self.errc_vec, errc_format));
            if errc_format == Self::ERRC_PF {
                info!("> cr2 (PF addr) = {:#018x}", unsafe {
                    archop::reg::cr2::read()
                });
            }
        }
        info!("> Code addr  = {:#018x}", self.rip);
        info!("> RFlags     = {}", Flags::new(self.rflags, Self::RFLAGS));

        info!("> GPRs: ");
        info!("  rax = {:#018x}, rcx = {:#018x}", self.rax, self.rcx);
        info!("  rdx = {:#018x}, rbx = {:#018x}", self.rdx, self.rbx);
        info!("  rbp = {:#018x}, rsp = {:#018x}", self.rbp, self.rsp);
        info!("  rsi = {:#018x}, rdi = {:#018x}", self.rsi, self.rdi);
        info!("  r8  = {:#018x}, r9  = {:#018x}", self.r8, self.r9);
        info!("  r10 = {:#018x}, r11 = {:#018x}", self.r10, self.r11);
        info!("  r12 = {:#018x}, r13 = {:#018x}", self.r12, self.r13);
        info!("  r14 = {:#018x}, r15 = {:#018x}", self.r14, self.r15);

        info!("> Segments:");
        info!("  cs  = {:#018x}, ss  = {:#018x}", self.cs, self.ss);
        info!("  fs_base = {:#018x}", self.fs_base);
        info!("  gs_base = {:#018x}", self.gs_base);
    }
}

/// # Safety
///
/// This function must be called only by assembly stubs.
#[no_mangle]
unsafe extern "C" fn save_regs() {
    if let Some(ref mut cur) = &mut *crate::sched::SCHED.current() {
        debug_assert!(!cur.running_state.not_running());
        cur.ext_frame.save();
    }
}

#[no_mangle]
pub(super) unsafe extern "C" fn switch_finishing() {
    if let Some(ref cur) = *crate::sched::SCHED.current() {
        log::trace!("Switched to task {:?}, P{}", cur.tid().raw(), PREEMPT.raw());
        debug_assert!(!cur.running_state.not_running());

        let tss_rsp0 = cur.kstack.top().val() as u64;
        KERNEL_GS.update_tss_rsp0(tss_rsp0);
        KERNEL_GS.update_tss_io_bitmap(cur.io_bitmap.as_deref());
        crate::mem::space::set_current(Arc::clone(cur.space.mem()));
        cur.ext_frame.load();
        if !cpu::arch::in_intr() && cur.tid.ty() == task::Type::Kernel {
            KERNEL_GS.load();
        }
    }
    PREEMPT.enable(None);
}

extern "C" {
    pub(super) fn switch_kframe(old: *mut *mut u8, new: *mut u8);

    fn task_fresh();
}
