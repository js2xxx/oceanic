pub mod apic;
pub mod intr;
pub mod seg;
pub mod syscall;
pub mod tsc;

use core::{
    cell::UnsafeCell,
    ptr::null_mut,
    sync::atomic::{AtomicUsize, Ordering},
};

use bitvec::slice::BitSlice;
use paging::LAddr;

pub use self::seg::reload_pls;
use crate::cpu::Lazy;

pub const MAX_CPU: usize = 256;
static CPU_INDEX: AtomicUsize = AtomicUsize::new(0);
static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

/// # Safety
///
/// This function is only called before architecture initialization.
pub unsafe fn set_id(bsp: bool) -> usize {
    use archop::msr;
    let id = CPU_INDEX.fetch_add(1, Ordering::SeqCst);
    msr::write(msr::TSC_AUX, id as u64);

    while !bsp && CPU_COUNT.load(Ordering::SeqCst) == 0 {
        core::hint::spin_loop();
    }
    id
}

/// # Safety
///
/// This function is only called after [`set_id`].
#[inline]
pub unsafe fn id() -> usize {
    archop::msr::rdtscp().1 as usize
}

#[inline]
pub fn count() -> usize {
    CPU_COUNT.load(Ordering::Relaxed)
}

#[inline]
pub fn in_intr() -> bool {
    extern "C" {
        fn cpu_in_intr() -> u32;
    }
    unsafe { cpu_in_intr() != 0 }
}

/// # Safety
///
/// This function is only called after [`set_id`].
#[inline]
pub unsafe fn is_bsp() -> bool {
    id() == 0
}

#[repr(C)]
pub struct KernelGs {
    tss_rsp0: UnsafeCell<u64>,
    syscall_user_stack: *mut u8,
    syscall_stack: LAddr,
    kernel_fs: LAddr,
}

#[thread_local]
pub static KERNEL_GS: Lazy<KernelGs> = Lazy::new(|| KernelGs {
    tss_rsp0: UnsafeCell::new(unsafe { seg::ndt::TSS.rsp0() }),
    syscall_user_stack: null_mut(),
    syscall_stack: unsafe { syscall::init() }.expect("Memory allocation failed"),
    kernel_fs: LAddr::from(unsafe { archop::reg::read_fs() } as usize),
});

impl KernelGs {
    /// Load the object.
    ///
    /// This function consumes the object and transform it into a 'permanent'
    /// register into [`archop::msr::KERNEL_GS_BASE`] so that interrupt
    /// handlers can access data from it without receiving its object.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    ///
    /// The caller must ensure that this function is called only if
    /// [`archop::msr::KERNEL_GS_BASE`] is uninitialized.
    pub unsafe fn load(&self) {
        use archop::msr;
        msr::write(msr::KERNEL_GS_BASE, self.as_ptr() as u64);
    }

    /// Update TSS's rsp0 a.k.a. task's interrupt frame pointer.
    ///
    /// This function both sets the rsp0 and its backup in the kernel GS.
    ///
    /// # Safety
    ///
    /// `rsp0` must be a valid pointer to a task's interrupt frame.
    pub unsafe fn update_tss_rsp0(&self, rsp0: u64) {
        seg::ndt::TSS.set_rsp0(rsp0);
        *self.tss_rsp0.get() = rsp0;
    }

    /// Update TSS's I/O bitmap.
    ///
    /// # Safety
    ///
    /// `bitmap`'s length must equal to 65536.
    pub unsafe fn update_tss_io_bitmap(&self, bitmap: Option<&BitSlice>) {
        let ptr = seg::ndt::TSS.bitmap();
        if let Some(bitmap) = bitmap {
            (*ptr).copy_from_bitslice(bitmap);
        } else {
            let ptr = (*ptr).as_mut_raw_slice();
            ptr.fill(usize::MAX);
        }
    }

    #[inline]
    pub unsafe fn as_ptr(&self) -> *const u8 {
        (self as *const KernelGs).cast()
    }
}

/// Initialize x86_64 architecture.
///
/// Here we manually initialize [`CPU_COUNT`] for better performance.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from
/// bootstrap CPU.
pub unsafe fn init() {
    archop::fpu::init();

    seg::init();

    // SAFETY: During bootstrap initialization.
    unsafe { KERNEL_GS.load() };

    apic::init();

    let cnt = {
        let lapic_data = crate::dev::acpi::platform_info()
            .processor_info
            .as_ref()
            .expect("Failed to get LAPIC data");
        apic::ipi::start_cpus(&lapic_data.application_processors)
    };
    CPU_COUNT.store(cnt + 1, Ordering::SeqCst);
    intr::init();
}

/// Initialize x86_64 architecture.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from
/// each application CPU.
pub unsafe fn init_ap() {
    seg::init_ap();

    // SAFETY: During bootstrap initialization.
    unsafe { KERNEL_GS.load() };

    apic::init();
    intr::init();
}
