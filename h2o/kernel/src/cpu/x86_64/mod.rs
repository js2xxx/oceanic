pub mod apic;
pub mod intr;
pub mod seg;
pub mod syscall;
pub mod tsc;

use core::{
    mem::MaybeUninit,
    ptr::null_mut,
    sync::atomic::{AtomicUsize, Ordering},
};

use paging::LAddr;

pub const MAX_CPU: usize = 256;
pub static CPU_INDEX: AtomicUsize = AtomicUsize::new(0);
static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

/// # Safety
///
/// This function is only called before architecture initialization.
pub unsafe fn set_id(bsp: bool) -> usize {
    use archop::msr;
    let id = CPU_INDEX.fetch_add(1, Ordering::SeqCst);
    msr::write(msr::TSC_AUX, id as u64);

    while !bsp && count() == 0 {
        core::hint::spin_loop();
    }
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

pub fn in_intr() -> bool {
    extern "C" {
        fn cpu_in_intr() -> u32;
    }
    unsafe { cpu_in_intr() != 0 }
}

/// # Safety
///
/// This function is only called after [`set_id`].
pub unsafe fn is_bsp() -> bool {
    id() == 0
}

#[repr(C)]
pub struct KernelGs {
    tss_rsp0: u64,
    syscall_user_stack: *mut u8,
    syscall_stack: LAddr,
    kernel_fs: LAddr,
}

#[thread_local]
static mut KERNEL_GS: MaybeUninit<KernelGs> = MaybeUninit::uninit();

impl KernelGs {
    pub fn new(syscall_stack: LAddr, kernel_fs: LAddr) -> Self {
        KernelGs {
            tss_rsp0: unsafe { seg::ndt::TSS.rsp0() },
            syscall_user_stack: null_mut(),
            syscall_stack,
            kernel_fs,
        }
    }

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
    pub unsafe fn load(this: Self) {
        let ptr = KERNEL_GS.write(this) as *mut Self;

        use archop::msr;
        msr::write(msr::KERNEL_GS_BASE, ptr as u64);
    }

    pub unsafe fn reload() {
        let ptr = KERNEL_GS.as_mut_ptr();

        use archop::msr;
        msr::write(msr::KERNEL_GS_BASE, ptr as u64);
    }

    /// Update TSS's rsp0 a.k.a. task's interrupt frame pointer.
    ///
    /// This function both sets the rsp0 and its backup in the kernel GS.
    ///
    /// # Safety
    ///
    /// `rsp0` must be a valid pointer to a task's interrupt frame.
    pub unsafe fn update_tss_rsp0(rsp0: u64) {
        let this = KERNEL_GS.assume_init_mut();
        seg::ndt::TSS.set_rsp0(rsp0);
        this.tss_rsp0 = rsp0;
    }

    pub unsafe fn as_ptr() -> *mut u8 {
        KERNEL_GS.as_mut_ptr().cast()
    }
}

/// Initialize x86_64 architecture.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from
/// bootstrap CPU.
pub unsafe fn init() {
    archop::fpu::init();

    let platform_info = unsafe { crate::dev::acpi::platform_info() };

    let kernel_fs = seg::init();

    unsafe { tsc::init() };

    apic::init();

    let syscall_stack = syscall::init().expect("Memory allocation failed");

    let kernel_gs = KernelGs::new(syscall_stack, kernel_fs);
    // SAFE: During bootstrap initialization.
    unsafe { KernelGs::load(kernel_gs) };

    let cnt = {
        let lapic_data = platform_info
            .processor_info
            .as_ref()
            .expect("Failed to get LAPIC data");
        apic::ipi::start_cpus(&lapic_data.application_processors)
    };
    CPU_COUNT.store(cnt + 1, Ordering::SeqCst);
}

/// Initialize x86_64 architecture.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from
/// each application CPU.
pub unsafe fn init_ap() {
    let kernel_fs = seg::init_ap();

    apic::init();

    let syscall_stack = syscall::init().expect("Memory allocation failed");

    let kernel_gs = KernelGs::new(syscall_stack, kernel_fs);
    // SAFE: During bootstrap initialization.
    unsafe { KernelGs::load(kernel_gs) };
}
