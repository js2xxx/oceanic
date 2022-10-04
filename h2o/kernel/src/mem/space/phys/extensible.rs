use alloc::{
    alloc::Global,
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use core::{
    alloc::{AllocError, Allocator, Layout},
    slice,
};

use bitop_ex::BitOpEx;
use paging::{LAddr, PAddr, PAGE_SHIFT, PAGE_SIZE};

use crate::{
    sched::{Arsc, BasicEvent, Event},
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
            let len = block.len.saturating_sub(offset);
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

    fn shrink(&mut self, new_len: usize) {
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

    #[allow(dead_code)]
    fn resize(&mut self, new_len: usize, zeroed: bool) -> Result<(), AllocError> {
        if self.len < new_len {
            self.extend(new_len, zeroed)
        } else {
            self.shrink(new_len);
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct Phys {
    offset: usize,
    len: usize,
    inner: Arsc<PhysInner>,
    event: Arc<BasicEvent>,
}

pub type PinnedPhys = Phys;

impl Phys {
    pub fn allocate(len: usize, zeroed: bool) -> Result<Self, AllocError> {
        PhysInner::allocate(len, zeroed).and_then(|inner| {
            Ok(Phys {
                offset: 0,
                len: inner.len,
                inner: Arsc::try_new(inner)?,
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
    pub fn pin(this: Self) -> PinnedPhys {
        this
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
                let (dst, _) = child.inner.iter().next().expect("Inconsistent map");
                let mut dst = *dst.to_laddr(minfo::ID_OFFSET);

                for (src, sl) in self.inner.range(new_offset, len) {
                    let src = *src.to_laddr(minfo::ID_OFFSET);
                    unsafe {
                        dst.copy_from_nonoverlapping(src, sl);
                        dst = dst.add(sl);
                    }
                }
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

    pub fn map_iter(&self, offset: usize, len: usize) -> impl Iterator<Item = (PAddr, usize)> + '_ {
        self.inner.range(self.offset + offset, len)
    }

    pub fn read(
        &self,
        offset: usize,
        len: usize,
        buffer: UserPtr<Out, u8>,
    ) -> sv_call::Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);

        let mut buffer = buffer;
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

    pub fn write(
        &self,
        offset: usize,
        len: usize,
        buffer: UserPtr<In, u8>,
    ) -> sv_call::Result<usize> {
        let offset = self.len.min(offset);
        let len = self.len.saturating_sub(offset).min(len);

        let mut buffer = buffer;
        for (base, len) in self.inner.range(self.offset + offset, len) {
            let dst = *base.to_laddr(minfo::ID_OFFSET);
            unsafe {
                buffer.read_slice(dst, len)?;
                buffer = UserPtr::new(buffer.as_ptr().add(len));
            }
        }
        Ok(len)
    }
}

impl PartialEq for Phys {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.len == other.len
            && Arsc::ptr_eq(&self.inner, &other.inner)
    }
}
