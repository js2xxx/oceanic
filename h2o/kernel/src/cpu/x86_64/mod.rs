pub mod apic;
pub mod intr;
pub mod seg;
pub mod syscall;

use crate::mem::space::Space;

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::pin::Pin;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_CPU: usize = 256;
pub static CPU_INDEX: AtomicUsize = AtomicUsize::new(0);

/// # Safety
///
/// This function is only called before architecture initialization.
pub unsafe fn set_id() -> usize {
      use archop::msr;
      let id = CPU_INDEX.fetch_add(1, Ordering::AcqRel);
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

#[repr(C)]
pub struct KernelGs<'a> {
      save_regs: *mut u8,
      tss_rsp0: *mut u8,
      syscall_user_stack: *mut u8,
      syscall_stack: *mut u8,

      lapic: apic::Lapic<'a>,
}

impl<'a> KernelGs<'a> {
      pub fn new(tss_rsp0: *mut u8, syscall_stack: *mut u8, lapic: apic::Lapic<'a>) -> Self {
            KernelGs {
                  save_regs: intr::ctx::test::save_regs as *mut u8,
                  tss_rsp0,
                  syscall_user_stack: null_mut(),
                  syscall_stack,

                  lapic,
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
            intr::ctx::test::init_stack_top(
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
      pub unsafe fn access<'b>() -> &'b KernelGs<'b> {
            use archop::msr;
            let ptr = msr::read(msr::KERNEL_GS_BASE) as *const KernelGs<'_>;
            &*ptr
      }

      /// # Safety
      ///
      /// The caller must ensure that this function is called inside an interrupt handler and
      /// there's an `KernelGs` object stored in [`archop::msr::GS_BASE`].
      pub unsafe fn access_in_intr<'b>() -> &'b mut KernelGs<'b> {
            use archop::msr;
            let ptr = msr::read(msr::GS_BASE) as *mut KernelGs<'_>;
            &mut *ptr
      }
}

/// Initialize x86_64 architecture.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from bootstrap
/// CPU.
pub unsafe fn init(
      space: &Arc<Space>,
      lapic_data: acpi::table::madt::LapicData,
      ioapic_data: acpi::table::madt::IoapicData,
) -> (
      spin::Mutex<seg::ndt::DescTable<'_>>,
      Pin<&mut seg::ndt::TssStruct>,
) {
      let (gdt, tss) = seg::init(space);

      let acpi::table::madt::LapicData {
            ty: lapic_ty,
            lapics,
      } = lapic_data;
      let mut lapic = apic::Lapic::new(lapic_ty, space);
      lapic.enable();
      let lapic = lapic.activate_timer(apic::timer::TimerMode::Periodic, 7, 256);

      let syscall_stack = syscall::init();

      let kernel_gs = KernelGs::new(*tss.rsp0(), syscall_stack, lapic);
      // SAFE: During bootstrap initialization.
      unsafe { kernel_gs.load() };

      (gdt, tss)
}
