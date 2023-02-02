#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VTable {
    pub global_exe: *const solvent_async::exe::Executor,
    /// We don't make this thread-local because every !send task only resides in
    /// another !send one, and thus only resides in the main task, meaning only
    /// one thread can execute thread-local tasks.
    pub local_exe: *const solvent_async::exe::LocalExecutor,
    pub local_fs: *const solvent_fs::fs::LocalFs,

    pub alloc: unsafe extern "C" fn(usize, usize) -> *mut (),
    pub dealloc: unsafe extern "C" fn(*mut (), usize, usize),
}

#[cfg(feature = "ddk")]
mod ddk {
    use core::sync::atomic;

    use solvent_async::exe::{Executor, LocalExecutor};
    use solvent_fs::fs::LocalFs;

    use super::*;

    static mut VTABLE: Option<VTable> = None;

    pub(crate) fn vtable() -> &'static VTable {
        unsafe { VTABLE.as_ref().expect("DDK vtable uninitialized") }
    }

    pub fn global_executor() -> &'static Executor {
        unsafe { &*vtable().global_exe }
    }

    pub fn local_executor<T, F: FnOnce(&LocalExecutor) -> T>(f: F) -> T {
        f(unsafe { &*vtable().local_exe })
    }

    pub fn local_fs() -> &'static LocalFs {
        unsafe { &*vtable().local_fs }
    }

    /// # Safety
    ///
    /// This function must be called from `__h2o_ddk_enter` only once before
    /// everything of the driver is initialized.
    pub unsafe fn __h2o_ddk_init(vtable: *const VTable) {
        VTABLE = Some(vtable.read());
        dbglog::init(log::Level::Debug);
        atomic::fence(atomic::Ordering::Acquire);
    }

    /// # Safety
    ///
    /// This function must be called  only after every async task of the driver
    /// is dropped.
    #[no_mangle]
    unsafe extern "C" fn __h2o_ddk_exit() {
        VTABLE = None;
    }

    /// Set the entry of the driver.
    ///
    /// The init function should be signatured `async fn(Channel)`.
    #[macro_export]
    macro_rules! entry {
        ($init:ident) => {
            #[allow(improper_ctypes)]
            #[no_mangle]
            unsafe extern "C" fn __h2o_ddk_enter(
                vtable: *const $crate::ffi::VTable,
                instance: solvent::obj::Handle,
            ) -> *mut () {
                $crate::ffi::__h2o_ddk_init(vtable);

                let instance = <solvent::ipc::Channel as solvent::obj::Object>::from_raw(instance);

                struct AssertInit<F: core::future::Future<Output = ()> + 'static>(F);
                let assert_init = AssertInit(($init)(instance));

                let task = $crate::ffi::local_executor(|exe| exe.spawn(assert_init.0));
                let task = alloc::boxed::Box::new(task);
                alloc::boxed::Box::into_raw(task).cast()
            }
        };
    }

    #[panic_handler]
    fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
        log::error!("{}", info);

        loop {
            unsafe { core::arch::asm!("pause; ud2") }
        }
    }
}

#[cfg(feature = "ddk")]
pub use ddk::*;
