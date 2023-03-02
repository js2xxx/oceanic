use alloc::{
    alloc::Global,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    alloc::{Allocator, Layout},
    slice,
};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};
use sv_call::{Result, EPERM};

use super::PhysTrait;
use crate::{
    sched::{Arsc, BasicEvent, Event},
    syscall::{In, Out, UserPtr},
};

#[derive(Debug)]
struct PhysInner {
    from_allocator: bool,
    base: PAddr,
    size: usize,
}

impl PhysInner {
    unsafe fn new_manual(from_allocator: bool, base: PAddr, size: usize) -> PhysInner {
        PhysInner {
            from_allocator,
            base,
            size,
        }
    }
}

impl Drop for PhysInner {
    fn drop(&mut self) {
        if self.from_allocator {
            let ptr = unsafe { self.base.to_laddr(minfo::ID_OFFSET).as_non_null_unchecked() };
            let layout =
                unsafe { Layout::from_size_align_unchecked(self.size, PAGE_SIZE) }.pad_to_align();
            unsafe { Global.deallocate(ptr, layout) };
        }
    }
}

#[derive(Debug, Clone)]
pub struct Phys {
    offset: usize,
    len: usize,
    inner: Arsc<PhysInner>,
}

impl From<Arsc<PhysInner>> for Phys {
    fn from(inner: Arsc<PhysInner>) -> Self {
        Phys {
            offset: 0,
            len: inner.size,
            inner,
        }
    }
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, size: usize) -> Result<Self> {
        unsafe { Arsc::try_new(PhysInner::new_manual(false, base, size)) }
            .map_err(sv_call::Error::from)
            .map(Self::from)
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted or the size is zero.
    pub fn allocate(size: usize, zeroed: bool) -> Result<Self> {
        if size == 0 {
            return Err(sv_call::ENOMEM);
        }
        let alloc_size = size.round_up_bit(PAGE_SHIFT);

        let mut inner = Arsc::try_new_uninit()?;
        let layout = unsafe { Layout::from_size_align_unchecked(alloc_size, PAGE_SIZE) }.pad_to_align();
        let mem = if zeroed {
            Global.allocate_zeroed(layout)
        } else {
            Global.allocate(layout)
        };

        mem.map(|ptr| unsafe {
            Arsc::get_mut_unchecked(&mut inner).write(PhysInner::new_manual(
                true,
                LAddr::from(ptr).to_paddr(minfo::ID_OFFSET),
                size,
            ));
            Arsc::assume_init(inner)
        })
        .map_err(sv_call::Error::from)
        .map(Self::from)
    }

    fn raw(&self) -> *mut u8 {
        unsafe { self.inner.base.to_laddr(minfo::ID_OFFSET).add(self.offset) }
    }
}

impl PartialEq for Phys {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.len == other.len
            && Arsc::ptr_eq(&self.inner, &other.inner)
    }
}

impl PhysTrait for Phys {
    fn event(&self) -> Weak<dyn Event> {
        Weak::<BasicEvent>::new()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn pin(&self, offset: usize, len: usize, _: bool) -> Result<Vec<(PAddr, usize)>> {
        let base = PAddr::new(*self.inner.base + self.offset + offset);
        let len = self.len.saturating_sub(offset).min(len);
        Ok((len > 0).then_some((base, len)).into_iter().collect())
    }

    fn unpin(&self, _: usize, _: usize) {}

    fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Arc<super::Phys>> {
        if offset.contains_bit(PAGE_SHIFT) {
            return Err(sv_call::EALIGN);
        }

        let new_offset = self.offset.wrapping_add(offset);
        let end = new_offset.wrapping_add(len);
        if self.offset <= new_offset && new_offset < end && end <= self.offset + self.len {
            let mut ret = Arc::try_new_uninit()?;
            let phys = if copy {
                let child = Self::allocate(len, true)?;
                let dst = child.raw();
                unsafe {
                    let src = self.raw().add(offset);
                    dst.copy_from_nonoverlapping(src, len);
                }
                child
            } else {
                Phys {
                    offset: new_offset,
                    len,
                    inner: Arsc::clone(&self.inner),
                }
            };
            Arc::get_mut(&mut ret).unwrap().write(phys.into());
            Ok(unsafe { ret.assume_init() })
        } else {
            Err(sv_call::ERANGE)
        }
    }

    fn base(&self) -> PAddr {
        PAddr::new(*self.inner.base + self.offset)
    }

    fn resize(&self, _: usize, _: bool) -> Result {
        Err(EPERM)
    }

    fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out>) -> Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);
        unsafe {
            let ptr = self.raw().add(offset);
            let slice = slice::from_raw_parts(ptr, len);
            buffer.write_slice(slice)?;
        }
        Ok(len)
    }

    fn write(&self, offset: usize, len: usize, buffer: UserPtr<In>) -> Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);
        unsafe {
            let ptr = self.raw().add(offset);
            buffer.read_slice(ptr, len)?;
        }
        Ok(len)
    }

    fn read_vectored(&self, mut offset: usize, bufs: &[(UserPtr<Out>, usize)]) -> Result<usize> {
        let mut read_len = 0;
        for buf in bufs {
            let actual_offset = self.len.min(offset);
            let len = self.len.saturating_sub(actual_offset).min(buf.1);

            let buffer = buf.0.out();
            unsafe {
                let ptr = self.raw().add(offset);
                let slice = slice::from_raw_parts(ptr, len);
                buffer.write_slice(slice)?;
            }
            read_len += len;
            offset += len;
            if len < buf.1 {
                break;
            }
        }
        Ok(read_len)
    }

    fn write_vectored(&self, mut offset: usize, bufs: &[(UserPtr<In>, usize)]) -> Result<usize> {
        let mut written_len = 0;
        for buf in bufs {
            let actual_offset = self.len.min(offset);
            let len = self.len.saturating_sub(actual_offset).min(buf.1);

            let buffer = buf.0.r#in();
            unsafe {
                let ptr = self.raw().add(offset);
                buffer.read_slice(ptr, len)?;
            }
            written_len += len;
            offset += len;
            if len < buf.1 {
                break;
            }
        }
        Ok(written_len)
    }
}
