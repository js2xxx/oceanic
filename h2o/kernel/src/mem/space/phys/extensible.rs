use alloc::{
    alloc::Global,
    collections::BTreeMap,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    alloc::{AllocError, Allocator, Layout},
    slice,
};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};
use spin::RwLock;
use sv_call::{
    ipc::{SIG_READ, SIG_WRITE},
    EAGAIN, EPERM,
};

use super::PhysTrait;
use crate::{
    sched::{Arsc, BasicEvent, Event, PREEMPT},
    syscall::{In, Out, UserPtr},
};

#[derive(Debug)]
struct Block {
    from_allocator: bool,
    base: PAddr,
    len: usize,
    capacity: usize,
}

impl Block {
    unsafe fn new_manual(from_allocator: bool, base: PAddr, len: usize, capacity: usize) -> Block {
        Block {
            from_allocator,
            base,
            len,
            capacity,
        }
    }

    fn allocate(len: usize, zeroed: bool) -> Result<Block, AllocError> {
        let capacity = len.round_up_bit(PAGE_SHIFT);
        let layout = unsafe { Layout::from_size_align_unchecked(capacity, PAGE_SIZE) };
        let memory = if zeroed {
            Global.allocate_zeroed(layout)
        } else {
            Global.allocate(layout)
        }?;
        Ok(unsafe {
            Block::new_manual(
                true,
                LAddr::from(memory).to_paddr(minfo::ID_OFFSET),
                len,
                capacity,
            )
        })
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        if self.from_allocator {
            let ptr = unsafe { self.base.to_laddr(minfo::ID_OFFSET).as_non_null_unchecked() };
            let layout = unsafe { Layout::from_size_align_unchecked(self.capacity, PAGE_SIZE) }
                .pad_to_align();
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
        let mut map = BTreeMap::new();

        let mut acc = len;
        let mut offset = 0;
        while acc > PAGE_SIZE {
            let part = 1 << (usize::BITS - acc.leading_zeros() - 1);

            let new = Block::allocate(part, zeroed)?;
            map.insert(offset, new);

            offset += part;
            acc -= part;
        }
        if acc > 0 {
            let new = Block::allocate(acc, zeroed)?;
            map.insert(offset, new);
        }
        Ok(PhysInner { map, len })
    }

    fn truncate(&mut self, new_len: usize) {
        self.map.split_off(&new_len);
        if let Some(mut ent) = self.map.last_entry() {
            if *ent.key() < new_len && ent.get().len + ent.key() > new_len {
                ent.get_mut().len = new_len - ent.key();
            }
        }
        self.len = new_len;
    }

    fn extend(&mut self, new_len: usize, zeroed: bool) -> Result<(), AllocError> {
        let start = self.len;
        let mut len = new_len.saturating_sub(start);
        if let Some(mut last) = self.map.last_entry() {
            let delta = (last.get().capacity - last.get().len).min(len);
            len -= delta;
            last.get_mut().len += delta;
            self.len += delta;
        }
        if len > 0 {
            let new = Block::allocate(len, zeroed)?;
            self.map.insert(self.len, new);
            self.len += len;
        }
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
pub struct Static {
    offset: usize,
    len: usize,
    inner: Arsc<PhysInner>,
}

impl Static {
    pub fn allocate(len: usize, zeroed: bool) -> Result<Self, AllocError> {
        PhysInner::allocate(len, zeroed).and_then(|inner| {
            Ok(Static {
                offset: 0,
                len: inner.len,
                inner: Arsc::try_new(inner)?,
            })
        })
    }
}

impl PartialEq for Static {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.len == other.len
            && Arsc::ptr_eq(&self.inner, &other.inner)
    }
}

impl PhysTrait for Static {
    fn event(&self) -> Weak<dyn Event> {
        Weak::<BasicEvent>::new()
    }
    fn len(&self) -> usize {
        self.len
    }

    fn pin(&self, offset: usize, len: usize, _: bool) -> sv_call::Result<Vec<(PAddr, usize)>> {
        Ok(self.inner.range(self.offset + offset, len).collect())
    }

    fn create_sub(
        &self,
        offset: usize,
        len: usize,
        copy: bool,
    ) -> sv_call::Result<Arc<super::Phys>> {
        if offset.contains_bit(PAGE_SHIFT) || len.contains_bit(PAGE_SHIFT) {
            return Err(sv_call::EALIGN);
        }
        let cloned = Arsc::clone(&self.inner);

        let new_offset = self.offset.wrapping_add(offset);
        let end = new_offset.wrapping_add(len);
        if self.offset <= new_offset && new_offset < end && end <= self.offset + self.len {
            let mut ret = Arc::try_new_uninit()?;
            let phys = if copy {
                let child = Self::allocate(len, false)?;

                let (dst, _) = child.inner.iter().next().expect("Inconsistent map");
                let mut dst = *dst.to_laddr(minfo::ID_OFFSET);

                for (src, sl) in self.inner.range(new_offset, len) {
                    let src = *src.to_laddr(minfo::ID_OFFSET);
                    unsafe {
                        dst.copy_from_nonoverlapping(src, sl);
                        dst = dst.add(sl);
                    }
                }

                child
            } else {
                Static {
                    offset: new_offset,
                    len,
                    inner: cloned,
                }
            };
            Arc::get_mut(&mut ret).unwrap().write(phys.into());
            Ok(unsafe { ret.assume_init() })
        } else {
            Err(sv_call::ERANGE)
        }
    }

    fn base(&self) -> PAddr {
        unimplemented!("Extensible phys have multiple bases")
    }

    fn resize(&self, _: usize, _: bool) -> sv_call::Result {
        Err(EPERM)
    }

    fn read(&self, offset: usize, len: usize, mut buffer: UserPtr<Out>) -> sv_call::Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);

        for (base, len) in self.inner.range(self.offset + offset, len) {
            let src = *base.to_laddr(minfo::ID_OFFSET);
            unsafe {
                let src = slice::from_raw_parts(src, len);
                buffer.write_slice(src)?;
                buffer = UserPtr::new(buffer.as_ptr().add(len));
            }
        }
        Ok(len)
    }

    fn write(&self, offset: usize, len: usize, mut buffer: UserPtr<In>) -> sv_call::Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);

        for (base, len) in self.inner.range(self.offset + offset, len) {
            let dst = *base.to_laddr(minfo::ID_OFFSET);
            unsafe {
                buffer.read_slice(dst, len)?;
                buffer = UserPtr::new(buffer.as_ptr().add(len));
            }
        }
        Ok(len)
    }

    fn read_vectored(
        &self,
        mut offset: usize,
        bufs: &[(UserPtr<Out>, usize)],
    ) -> sv_call::Result<usize> {
        let mut read_len = 0;

        for buf in bufs {
            let actual_offset = self.len.min(offset);
            let len = self.len.saturating_sub(actual_offset).min(buf.1);

            let mut buffer = buf.0.out();
            for (base, len) in self.inner.range(self.offset + actual_offset, len) {
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

        Ok(read_len)
    }

    fn write_vectored(
        &self,
        mut offset: usize,
        bufs: &[(UserPtr<In>, usize)],
    ) -> sv_call::Result<usize> {
        let mut written_len = 0;

        for buf in bufs {
            let actual_offset = self.len.min(offset);
            let len = self.len.saturating_sub(actual_offset).min(buf.1);

            let mut buffer = buf.0.r#in();
            for (base, len) in self.inner.range(self.offset + actual_offset, len) {
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

        Ok(written_len)
    }
}

#[derive(Debug, Clone)]
pub struct Dynamic {
    inner: Arsc<RwLock<PhysInner>>,
    event: Arc<BasicEvent>,
}

impl Dynamic {
    pub fn allocate(len: usize, zeroed: bool) -> Result<Self, AllocError> {
        PhysInner::allocate(len, zeroed).and_then(|inner| {
            Ok(Dynamic {
                inner: Arsc::try_new(RwLock::new(inner))?,
                event: BasicEvent::new(0),
            })
        })
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
}

impl PartialEq for Dynamic {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Arsc::ptr_eq(&self.inner, &other.inner)
    }
}

impl PhysTrait for Dynamic {
    fn event(&self) -> Weak<dyn Event> {
        Arc::downgrade(&self.event) as _
    }

    fn len(&self) -> usize {
        // FIXME: For now, just let this slip.
        unsafe { (*self.inner.as_mut_ptr()).len }
    }

    fn pin(&self, offset: usize, len: usize, _: bool) -> sv_call::Result<Vec<(PAddr, usize)>> {
        let list = self.inner.read();
        Ok(list.range(offset, len).collect())
    }

    fn create_sub(&self, _: usize, _: usize, _: bool) -> sv_call::Result<Arc<super::Phys>> {
        Err(EPERM)
    }

    fn base(&self) -> PAddr {
        unimplemented!("Extensible phys have multiple bases")
    }

    fn resize(&self, new_len: usize, zeroed: bool) -> sv_call::Result {
        PREEMPT.scope(|| {
            let mut this = self.inner.try_write().ok_or(EAGAIN)?;
            this.resize(new_len, zeroed)?;
            Ok::<_, sv_call::Error>(())
        })?;
        self.notify_write();
        Ok(())
    }

    fn read(&self, offset: usize, len: usize, mut buffer: UserPtr<Out>) -> sv_call::Result<usize> {
        let len = PREEMPT.scope(|| {
            let this = self.inner.try_read().ok_or(EAGAIN)?;

            let offset = this.len.min(offset);
            let len = this.len.saturating_sub(offset).min(len);

            for (base, len) in this.range(offset, len) {
                let src = *base.to_laddr(minfo::ID_OFFSET);
                unsafe {
                    let src = slice::from_raw_parts(src, len);
                    buffer.write_slice(src)?;
                    buffer = UserPtr::new(buffer.as_ptr().add(len));
                }
            }
            Ok::<_, sv_call::Error>(len)
        })?;
        self.notify_read();
        Ok(len)
    }

    fn write(&self, offset: usize, len: usize, mut buffer: UserPtr<In>) -> sv_call::Result<usize> {
        let len = PREEMPT.scope(|| {
            let this = self.inner.try_write().ok_or(EAGAIN)?;

            let offset = this.len.min(offset);
            let len = this.len.saturating_sub(offset).min(len);

            for (base, len) in this.range(offset, len) {
                let dst = *base.to_laddr(minfo::ID_OFFSET);
                unsafe {
                    buffer.read_slice(dst, len)?;
                    buffer = UserPtr::new(buffer.as_ptr().add(len));
                }
            }
            Ok::<_, sv_call::Error>(len)
        })?;
        self.notify_write();
        Ok(len)
    }

    fn read_vectored(
        &self,
        mut offset: usize,
        bufs: &[(UserPtr<Out>, usize)],
    ) -> sv_call::Result<usize> {
        let mut read_len = 0;
        PREEMPT.scope(|| {
            let this = self.inner.try_read().ok_or(EAGAIN)?;
            let self_len = this.len;
            for buf in bufs {
                let actual_offset = self_len.min(offset);
                let len = self_len.saturating_sub(actual_offset).min(buf.1);

                let mut buffer = buf.0.out();
                for (base, len) in this.range(actual_offset, len) {
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

    fn write_vectored(
        &self,
        mut offset: usize,
        bufs: &[(UserPtr<In>, usize)],
    ) -> sv_call::Result<usize> {
        let mut written_len = 0;
        PREEMPT.scope(|| {
            let this = self.inner.try_write().ok_or(EAGAIN)?;
            let self_len = this.len;
            for buf in bufs {
                let actual_offset = self_len.min(offset);
                let len = self_len.saturating_sub(actual_offset).min(buf.1);

                let mut buffer = buf.0.r#in();
                for (base, len) in this.range(actual_offset, len) {
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
