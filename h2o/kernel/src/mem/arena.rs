mod atomic;

use core::{
    alloc::Layout,
    mem,
    ptr::NonNull,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering::*},
};

use static_assertions::const_assert_eq;

use self::atomic::AtomicDoubleU64;
use super::space::KernelVirt;
use crate::mem::space::{AllocType, Flags, KRL};

#[derive(Clone, Copy)]
#[repr(align(16))]
struct Node<T> {
    next: *mut Node<T>,
}
const_assert_eq!(mem::size_of::<Node<()>>(), 16);

pub struct Arena<T> {
    max_count: usize,
    base: NonNull<T>,
    head: AtomicDoubleU64,
    top: AtomicPtr<T>,
    off: usize,
    end: NonNull<T>,
    virt: KernelVirt,
    count: AtomicUsize,
}

unsafe impl<T: Send> Send for Arena<T> {}
unsafe impl<T: Send> Sync for Arena<T> {}

impl<T> Arena<T> {
    pub fn new(max_count: usize) -> Self {
        let (layout, off) = Layout::new::<T>()
            .align_to(16)
            .and_then(|layout| layout.repeat(max_count))
            .expect("Layout error");
        assert!(off >= 16);
        let virt = KRL
            .allocate_kernel(
                AllocType::Layout(layout),
                None,
                Flags::READABLE | Flags::WRITABLE,
            )
            .expect("Failed to allocate memory");

        let (base, end) = unsafe {
            let range = virt.range();
            (
                NonNull::new_unchecked(range.start.cast::<T>()),
                NonNull::new_unchecked(range.end.cast::<T>()),
            )
        };

        Arena {
            max_count,
            base,
            head: AtomicDoubleU64::default(),
            top: AtomicPtr::new(base.as_ptr()),
            off,
            end,
            virt,
            count: AtomicUsize::default(),
        }
    }

    pub fn allocate(&self) -> solvent::Result<NonNull<T>> {
        let mut head = self.head.load_acquire();
        let ptr = loop {
            let head_ptr = match NonNull::new(head.0 as *mut Node<T>) {
                Some(head) => head,
                None => break Err(solvent::Error::ENOMEM),
            };

            let next = unsafe { head_ptr.as_ref().next };
            match self
                .head
                .compare_exchange_acqrel(head, (next as u64, head.0 + 1))
            {
                Ok(_) => break Ok(head_ptr.cast::<T>()),
                Err(new) => head = new,
            }
        };

        ptr.or_else(|err| {
            let mut top = self.top.load(Acquire);
            loop {
                if top >= self.end.as_ptr() {
                    break Err(err);
                }

                let next = unsafe { top.cast::<u8>().add(self.off).cast() };
                match self.top.compare_exchange(top, next, AcqRel, Acquire) {
                    Ok(_) => break Ok(unsafe { NonNull::new_unchecked(top) }),
                    Err(new) => top = new,
                }
            }
        })
        .inspect(|_ptr| {
            self.count.fetch_add(1, SeqCst);
        })
    }

    /// # Safety
    ///
    /// The caller must ensure that `ptr` is previously allocated by this arena.
    pub unsafe fn deallocate(&self, ptr: NonNull<T>) -> solvent::Result {
        if !self.check_ptr(ptr) {
            return Err(solvent::Error::EINVAL);
        }

        let mut next = self.head.load_acquire();
        loop {
            let head = ptr.cast::<Node<T>>();
            // SAFETY: The safety is guaranteed by the notice.
            unsafe {
                head.as_ptr().write(Node {
                    next: next.0 as *mut Node<T>,
                })
            };

            match self
                .head
                .compare_exchange_acqrel(next, (head.as_ptr() as u64, next.1 + 1))
            {
                Ok(_) => {
                    self.count.fetch_sub(1, SeqCst);
                    return Ok(());
                }
                Err(new) => next = new,
            }
        }
    }

    #[inline]
    pub fn check_ptr(&self, ptr: NonNull<T>) -> bool {
        let top = self.top.load(Acquire);
        NonNull::new(top).map_or(false, |top| self.base <= ptr && ptr < top)
    }

    #[inline]
    pub fn check_index(&self, index: usize) -> bool {
        index < self.max_count
    }

    pub fn to_index(&self, ptr: NonNull<T>) -> solvent::Result<usize> {
        if self.check_ptr(ptr) {
            let base = self.base.as_ptr() as usize;
            let addr = ptr.as_ptr() as usize;
            let index = addr.wrapping_sub(base).wrapping_div(self.off);
            Some(index)
                .filter(|&index| self.check_index(index))
                .ok_or(solvent::Error::EINVAL)
        } else {
            Err(solvent::Error::EINVAL)
        }
    }

    pub fn from_index(&self, index: usize) -> solvent::Result<NonNull<T>> {
        if self.check_index(index) {
            let base = self.base.as_ptr() as usize;
            let addr = index.wrapping_mul(self.off).wrapping_add(base);
            NonNull::new(addr as *mut T)
                .filter(|&ptr| self.check_ptr(ptr))
                .ok_or(solvent::Error::EINVAL)
        } else {
            Err(solvent::Error::EINVAL)
        }
    }

    pub fn max_count(&self) -> usize {
        self.max_count
    }

    pub fn virt(&self) -> &KernelVirt {
        &self.virt
    }

    pub fn count(&self) -> usize {
        self.count.load(SeqCst)
    }
}
