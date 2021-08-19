pub mod apic;
pub mod intr;
pub mod seg;
pub mod syscall;
pub mod tsc;

use crate::sched::task::ctx;
use crate::dev::acpi;
use paging::LAddr;

use alloc::boxed::Box;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_CPU: usize = 256;
pub static CPU_INDEX: AtomicUsize = AtomicUsize::new(0);
static CPU_COUNT: AtomicUsize = AtomicUsize::new(1);

/// # Safety
///
/// This function is only called before architecture initialization.
pub unsafe fn set_id() -> usize {
      use archop::msr;
      let id = CPU_INDEX.fetch_add(1, Ordering::SeqCst);
      msr::write(msr::TSC_AUX, id as u64);
      id
}

/// # Safety
///
/// This function is only called after [`set_id`].
pub unsafe fn id() -> usize {
      use archop::msr;
      msr::read(msr::TSC_AUX) as usize
}

pub fn count() -> usize {
      CPU_COUNT.load(Ordering::SeqCst)
}

#[repr(C)]
pub struct KernelGs {
      save_regs: *mut u8,
      tss_rsp0: LAddr,
      syscall_user_stack: *mut u8,
      syscall_stack: LAddr,
      kernel_fs: LAddr,
}

impl KernelGs {
      pub fn new(tss_rsp0: LAddr, syscall_stack: LAddr, kernel_fs: LAddr) -> Self {
            KernelGs {
                  save_regs: ctx::arch::test::save_regs as *mut u8,
                  tss_rsp0,
                  syscall_user_stack: null_mut(),
                  syscall_stack,
                  kernel_fs,
            }
      }

      /// Load the object.
      ///
      /// This function consumes the object and transform it into a 'permanent' register into
      /// [`archop::msr::KERNEL_GS_BASE`] so that interrupt handlers can access data from it
      /// without receiving its object.
      ///
      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      ///
      /// The caller must ensure that this function is called only if
      /// [`archop::msr::KERNEL_GS_BASE`] is uninitialized.
      pub unsafe fn load(self) {
            ctx::arch::test::init_stack_top(
                  alloc::alloc::alloc(paging::PAGE_LAYOUT).add(paging::PAGE_SIZE),
            );

            let ptr = Box::into_raw(box self);

            use archop::msr;
            msr::write(msr::KERNEL_GS_BASE, ptr as u64);
      }

      /// # Safety
      ///
      /// The caller must ensure that this function is called out of any interrupt handler
      /// and there's an `KernelGs` object stored in [`archop::msr::KERNEL_GS_BASE`].
      pub unsafe fn access<'b>() -> &'b KernelGs {
            use archop::msr;
            let ptr = msr::read(msr::KERNEL_GS_BASE) as *const KernelGs;
            &*ptr
      }

      /// # Safety
      ///
      /// The caller must ensure that this function is called inside an interrupt handler and
      /// there's an `KernelGs` object stored in [`archop::msr::GS_BASE`].
      pub unsafe fn access_in_intr<'b>() -> &'b mut KernelGs {
            use archop::msr;
            let ptr = msr::read(msr::GS_BASE) as *mut KernelGs;
            &mut *ptr
      }
}

/// Initialize x86_64 architecture.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from bootstrap
/// CPU.
pub unsafe fn init(lapic_data: acpi::table::madt::LapicData) {
      let (tss_rsp0, kernel_fs) = seg::init();

      unsafe { tsc::init() };

      let acpi::table::madt::LapicData {
            ty: lapic_ty,
            lapics,
      } = lapic_data;
      apic::init(lapic_ty);

      let syscall_stack = syscall::init().expect("Memory allocation failed");

      let kernel_gs = KernelGs::new(tss_rsp0, syscall_stack, kernel_fs);
      // SAFE: During bootstrap initialization.
      unsafe { kernel_gs.load() };

      let cnt = apic::ipi::start_cpus(lapics);
      CPU_COUNT.store(cnt, Ordering::SeqCst);
}

/// Initialize x86_64 architecture.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from each application
/// CPU.
pub unsafe fn init_ap(lapic_data: acpi::table::madt::LapicData) {
      let (tss_rsp0, kernel_fs) = seg::init_ap();

      apic::init(lapic_data.ty);

      let syscall_stack = syscall::init().expect("Memory allocation failed");

      let kernel_gs = KernelGs::new(tss_rsp0, syscall_stack, kernel_fs);
      // SAFE: During bootstrap initialization.
      unsafe { kernel_gs.load() };
}
