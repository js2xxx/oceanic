use alloc::{
    alloc::Global,
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use core::{
    alloc::{AllocError, Allocator, Layout},
    mem, slice,
};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};
use spin::RwLock;
use sv_call::{
    ipc::{SIG_READ, SIG_WRITE},
    EAGAIN,
};

use crate::{
    sched::{Arsc, BasicEvent, Event, PREEMPT},
    syscall::{In, InPtrType, Out, OutPtrType, UserPtr},
};

#[derive(Debug)]
struct Block {
    from_allocator: bool,
    base: PAddr,
    len: usize,
    capacity: usize,
}

impl Block {
    unsafe fn new_manual(from_allocator: bool, base: PAddr, len: usize) -> Block {
        Block {
            from_allocator,
            base,
            len,
            capacity: len,
        }
    }

    fn allocate(len: usize, zeroed: bool) -> Result<Block, AllocError> {
        let len = len.round_up_bit(PAGE_SHIFT);
        let layout = unsafe { Layout::from_size_align_unchecked(len, PAGE_SIZE) };
        let memory = if zeroed {
            Global.allocate_zeroed(layout)
        } else {
            Global.allocate(layout)
        }?;
        Ok(unsafe { Block::new_manual(true, LAddr::from(memory).to_paddr(minfo::ID_OFFSET), len) })
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        if self.from_allocator {
            let ptr = unsafe { self.base.to_laddr(minfo::ID_OFFSET).as_non_null_unchecked() };
            let layout =
                unsafe { Layout::from_size_align_unchecked(self.len, PAGE_SIZE) }.pad_to_align();
            unsafe { Global.deallocate(ptr, layout) };
        }
    }
}

#[derive(Debug)]
struct PhysInner {
    map: BTreeMap<usize, Block>,
    len: usize,
}

impl PhysInner {
    fn range(&self, offset: usize, len: usize) -> impl Iterator<Item = (PAddr, usize)> + '_ {
        let end = offset + len;
        let first = self.map.range(..offset).next_back();
        let first = first.and_then(|(&base, block)| {
            let offset = offset - base;
            let len = block.len.saturating_sub(offset).min(len);
            (len > 0).then_some((PAddr::new(*block.base + offset), len))
        });
        let next = self
            .map
            .range(offset..end)
            .filter_map(move |(&base, block)| {
                let len = block.len.min(end.saturating_sub(base));
                (len > 0).then_some((block.base, len))
            });
        first.into_iter().chain(next)
    }

    fn iter(&self) -> impl Iterator<Item = (PAddr, usize)> + '_ {
        self.map.values().map(|block| (block.base, block.len))
    }

    fn allocate(len: usize, zeroed: bool) -> Result<Self, AllocError> {
        if len == 0 {
            return Err(AllocError);
        }
        let len = len.round_up_bit(PAGE_SHIFT);

        let mut map = BTreeMap::new();

        let mut acc = len;
        let mut offset = 0;
        while acc > 0 {
            let part = 1 << (usize::BITS - acc.leading_zeros() - 1);

            let new = Block::allocate(part, zeroed)?;
            map.insert(offset, new);

            offset += part;
            acc -= part;
        }
        Ok(PhysInner { map, len })
    }

    fn truncate(&mut self, new_len: usize) {
        let mut removed = self.map.split_off(&new_len);
        if let Some((offset, mut last)) = removed.pop_first() {
            drop(removed);
            if offset < new_len {
                last.len = new_len - offset;
                self.map.insert(offset, last);
            }
        }
        self.len = new_len;
    }

    fn extend(&mut self, new_len: usize, zeroed: bool) -> Result<(), AllocError> {
        let new_len = new_len.round_up_bit(PAGE_SHIFT);
        let start = self.len;
        let mut len = new_len.saturating_sub(start);
        if let Some(mut last) = self.map.last_entry() {
            if last.get().len < last.get().capacity {
                last.get_mut().len = last.get().capacity;
                len -= (last.get().capacity - last.get().len).min(len);
            }
        }
        let new = Block::allocate(len, zeroed)?;
        self.map.insert(self.len, new);
        self.len = new_len;
        Ok(())
    }

    fn resize(&mut self, new_len: usize, zeroed: bool) -> Result<(), AllocError> {
        if self.len < new_len {
            self.extend(new_len, zeroed)
        } else {
            self.truncate(new_len);
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct Phys {
    offset: usize,
    len: usize,
    inner: Arsc<RwLock<PhysInner>>,
    event: Arc<BasicEvent>,
}

impl Phys {
    pub fn allocate(len: usize, zeroed: bool) -> Result<Self, AllocError> {
        PhysInner::allocate(len, zeroed).and_then(|inner| {
            Ok(Phys {
                offset: 0,
                len: inner.len,
                inner: Arsc::try_new(RwLock::new(inner))?,
                event: BasicEvent::new(0),
            })
        })
    }

    pub fn event(&self) -> Weak<dyn Event> {
        Arc::downgrade(&self.event) as _
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn pin(this: Self) -> sv_call::Result<PinnedPhys> {
        mem::forget(this.inner.try_read().ok_or(EAGAIN)?);
        Ok(PinnedPhys(this))
    }

    fn notify_read(&self) {
        if self.inner.reader_count() > 0 {
            self.event.notify(0, SIG_READ);
        } else {
            self.event.notify(0, SIG_READ | SIG_WRITE);
        }
    }

    fn notify_write(&self) {
        self.event.notify(0, SIG_READ | SIG_WRITE);
    }

    pub fn resize(&self, new_len: usize, zeroed: bool) -> sv_call::Result {
        PREEMPT.scope(|| {
            let mut this = self.inner.try_write().ok_or(EAGAIN)?;
            this.resize(new_len, zeroed)?;
            Ok::<_, sv_call::Error>(())
        })?;
        self.notify_write();
        Ok(())
    }

    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> sv_call::Result<Self> {
        if offset.contains_bit(PAGE_SHIFT) || len.contains_bit(PAGE_SHIFT) {
            return Err(sv_call::EALIGN);
        }

        let new_offset = self.offset.wrapping_add(offset);
        let end = new_offset.wrapping_add(len);
        if self.offset <= new_offset && new_offset < end && end <= self.offset + self.len {
            if copy {
                let child = Self::allocate(len, false)?;

                PREEMPT.scope(|| {
                    let child = child.inner.read();
                    let (dst, _) = child.iter().next().expect("Inconsistent map");
                    let mut dst = *dst.to_laddr(minfo::ID_OFFSET);

                    let this = self.inner.try_read().ok_or(EAGAIN)?;
                    for (src, sl) in this.range(new_offset, len) {
                        let src = *src.to_laddr(minfo::ID_OFFSET);
                        unsafe {
                            dst.copy_from_nonoverlapping(src, sl);
                            dst = dst.add(sl);
                        }
                    }
                    Ok::<_, sv_call::Error>(())
                })?;
                self.notify_read();

                Ok(child)
            } else {
                Ok(Phys {
                    offset: new_offset,
                    len,
                    inner: Arsc::clone(&self.inner),
                    event: Arc::clone(&self.event),
                })
            }
        } else {
            Err(sv_call::ERANGE)
        }
    }

    pub fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out>) -> sv_call::Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);

        let mut buffer = buffer;
        PREEMPT.scope(|| {
            let this = self.inner.try_read().ok_or(EAGAIN)?;
            for (base, len) in this.range(self.offset + offset, len) {
                let src = *base.to_laddr(minfo::ID_OFFSET);
                unsafe {
                    let src = slice::from_raw_parts(src, len);
                    buffer.write_slice(src)?;
                    buffer = UserPtr::new(buffer.as_ptr().add(len));
                }
            }
            Ok::<_, sv_call::Error>(())
        })?;
        self.notify_read();
        Ok(len)
    }

    pub fn write(&self, offset: usize, len: usize, buffer: UserPtr<In>) -> sv_call::Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);

        let mut buffer = buffer;
        PREEMPT.scope(|| {
            let this = self.inner.try_write().ok_or(EAGAIN)?;
            for (base, len) in this.range(self.offset + offset, len) {
                let dst = *base.to_laddr(minfo::ID_OFFSET);
                unsafe {
                    buffer.read_slice(dst, len)?;
                    buffer = UserPtr::new(buffer.as_ptr().add(len));
                }
            }
            Ok::<_, sv_call::Error>(())
        })?;
        self.notify_write();
        Ok(len)
    }

    pub fn read_vectored<T: OutPtrType>(
        &self,
        mut offset: usize,
        bufs: &[(UserPtr<T>, usize)],
    ) -> sv_call::Result<usize> {
        let mut read_len = 0;
        PREEMPT.scope(|| {
            let this = self.inner.try_read().ok_or(EAGAIN)?;
            for buf in bufs {
                let actual_offset = self.len.min(offset);
                let len = self.len.saturating_sub(actual_offset).min(buf.1);

                let mut buffer = buf.0.out();
                for (base, len) in this.range(self.offset + actual_offset, len) {
                    let src = *base.to_laddr(minfo::ID_OFFSET);
                    unsafe {
                        let src = slice::from_raw_parts(src, len);
                        buffer.write_slice(src)?;
                        buffer = UserPtr::new(buffer.as_ptr().add(len));
                    }
                }
                read_len += len;
                offset += len;
                if len < buf.1 {
                    break;
                }
            }
            Ok::<_, sv_call::Error>(())
        })?;
        self.notify_read();
        Ok(read_len)
    }

    pub fn write_vectored<T: InPtrType>(
        &self,
        mut offset: usize,
        bufs: &[(UserPtr<T>, usize)],
    ) -> sv_call::Result<usize> {
        let mut written_len = 0;
        PREEMPT.scope(|| {
            let this = self.inner.try_write().ok_or(EAGAIN)?;
            for buf in bufs {
                let actual_offset = self.len.min(offset);
                let len = self.len.saturating_sub(actual_offset).min(buf.1);

                let mut buffer = buf.0.r#in();
                for (base, len) in this.range(self.offset + actual_offset, len) {
                    let dst = *base.to_laddr(minfo::ID_OFFSET);
                    unsafe {
                        buffer.read_slice(dst, len)?;
                        buffer = UserPtr::new(buffer.as_ptr().add(len));
                    }
                }
                written_len += len;
                offset += len;
                if len < buf.1 {
                    break;
                }
            }
            Ok::<_, sv_call::Error>(())
        })?;
        self.notify_write();
        Ok(written_len)
    }
}

impl PartialEq for Phys {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.len == other.len
            && Arsc::ptr_eq(&self.inner, &other.inner)
    }
}

#[derive(Debug)]
pub struct PinnedPhys(Phys);

impl PinnedPhys {
    pub fn map_iter(&self, offset: usize, len: usize) -> impl Iterator<Item = (PAddr, usize)> + '_ {
        assert!(self.0.inner.writer_count() == 0 && self.0.inner.reader_count() > 0);
        unsafe { (*self.0.inner.as_mut_ptr()).range(self.0.offset + offset, len) }
    }
}

impl Drop for PinnedPhys {
    fn drop(&mut self) {
        assert!(self.0.inner.reader_count() > 0);
        unsafe { self.0.inner.force_read_decrement() }
    }
}
