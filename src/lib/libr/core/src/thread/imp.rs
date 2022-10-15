use alloc::boxed::Box;
use core::{mem, ptr::NonNull, time::Duration};

use solvent::{
    error::Result,
    prelude::{Flags, Phys, Virt, PAGE_SIZE},
    task::{exit, sleep, Task},
};

pub const DEFAULT_STACK_SIZE: usize = 256 * 1024;

pub struct Thread {
    inner: Task,
    stack: Virt,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    /// # Safety
    ///
    /// `func` must implements `Send` and has its lifetime checked.
    pub unsafe fn new(name: Option<&str>, stack: usize, func: Box<dyn FnOnce()>) -> Result<Self> {
        let stack = stack.max(DEFAULT_STACK_SIZE);

        let virt = svrt::root_virt().allocate(None, Virt::page_aligned(stack + 2 * PAGE_SIZE))?;
        struct Guard<'a>(&'a Virt);
        impl Drop for Guard<'_> {
            #[inline]
            fn drop(&mut self) {
                let _ = self.0.destroy();
            }
        }
        let guard = Guard(&virt);
        let phys = Phys::allocate(stack, Default::default())?;

        let range = virt.map_phys(
            Some(PAGE_SIZE),
            phys,
            Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS,
        )?;

        let stack = NonNull::new_unchecked(range.as_mut_ptr().add(range.len()));
        let entry = NonNull::new_unchecked(thread_func as _);

        let func = Box::into_raw(Box::new(func));
        let task = Task::exec(name, None, entry, stack, None, func as u64).inspect_err(|_| {
            let _ = Box::from_raw(func);
        })?;

        mem::forget(guard);
        return Ok(Thread {
            inner: task,
            stack: virt,
        });

        extern "C" fn thread_func(_: u64, arg: *mut u8) {
            unsafe {
                __libc_allocate_tcb();
                Box::from_raw(arg as *mut Box<dyn FnOnce()>)();
                __libc_deallocate_tcb();
                exit(0);
            }
        }
    }

    pub fn yield_now() {
        let res = sleep(Duration::ZERO);
        assert!(res.is_ok());
    }

    pub fn sleep(duration: Duration) {
        let res = sleep(duration);
        assert!(res.is_ok());
    }

    pub fn join(self) {
        let res = self.inner.join();
        assert_eq!(res, Ok(0), "Failed to join thread");
        let _ = self.stack.destroy();
    }
}

#[link(name = "ldso")]
extern "C" {
    fn __libc_allocate_tcb();

    fn __libc_deallocate_tcb();
}
