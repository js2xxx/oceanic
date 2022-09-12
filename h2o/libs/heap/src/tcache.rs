use core::{alloc::Layout, mem, ptr::NonNull};

use array_macro::array;
use paging::LAddr;
use spin::Mutex;

use crate::{
    alloc::Error,
    page::{self, Classes, NR_OBJ_SIZES},
    pool, unwrap_layout, OBJ_SIZES,
};

enum TcSlabSize {
    Small = 32,
    Medium = 8,
    Large = 4,
}

pub struct ThreadCache {
    slabs: [TcSlab; NR_OBJ_SIZES],
}

impl ThreadCache {
    pub const fn new() -> Self {
        ThreadCache {
            slabs: array![i => match Classes::from_index(i) {
                Classes::Small => TcSlab::new(TcSlabSize::Small, OBJ_SIZES[i]),
                Classes::Medium => TcSlab::new(TcSlabSize::Medium, OBJ_SIZES[i]),
                Classes::Large => TcSlab::new(TcSlabSize::Large, OBJ_SIZES[i]),
            }; NR_OBJ_SIZES],
        }
    }

    pub fn allocate(&mut self, layout: Layout, pool: &Mutex<pool::Pool>) -> Result<LAddr, Error> {
        let index = unwrap_layout(layout)?;
        self.slabs[index].pop(pool)
    }

    pub fn deallocate(
        &mut self,
        addr: LAddr,
        layout: Layout,
        pool: &Mutex<pool::Pool>,
    ) -> Result<Option<NonNull<page::Page>>, Error> {
        let index = unwrap_layout(layout)?;
        self.slabs[index].push(addr, pool)
    }
}

pub struct TcSlab {
    memory: Option<NonNull<LAddr>>,
    count: usize,
    size: usize,
    obj_size: usize,
}

impl TcSlab {
    const fn new(size: TcSlabSize, obj_size: usize) -> Self {
        TcSlab {
            memory: None,
            count: 0,
            size: size as usize,
            obj_size,
        }
    }

    fn memory(&mut self, pool: &Mutex<pool::Pool>) -> Result<NonNull<LAddr>, Error> {
        match self.memory {
            Some(memory) => Ok(memory),
            None => {
                let mut pool = pool.lock();
                let layout =
                    Layout::from_size_align(self.size * self.obj_size, mem::align_of::<LAddr>())
                        .unwrap();
                let memory = pool
                    .allocate(layout)
                    .ok()
                    .and_then(|addr| addr.as_non_null())
                    .ok_or(Error::Internal(
                        "Memory exhausted when allocating space for thread cache",
                    ))?
                    .cast();
                self.memory = Some(memory);
                Ok(memory)
            }
        }
    }

    fn pop(&mut self, pool: &Mutex<pool::Pool>) -> Result<LAddr, Error> {
        let memory = self.memory(pool)?;

        if self.count == 0 {
            let mut pool = pool.lock();
            let layout = Layout::from_size_align(self.obj_size, mem::align_of::<LAddr>()).unwrap();
            while self.count < self.size {
                let addr = match pool.allocate(layout) {
                    Ok(addr) => addr,
                    Err(_) => break,
                };

                unsafe { memory.as_ptr().add(self.count).write(addr) };
                self.count += 1;
            }
        }

        if self.count > 0 {
            self.count -= 1;
            let addr = unsafe { memory.as_ptr().add(self.count).read() };
            Ok(addr)
        } else {
            Err(Error::NeedExt)
        }
    }

    fn push(
        &mut self,
        addr: LAddr,
        pool: &Mutex<pool::Pool>,
    ) -> Result<Option<NonNull<page::Page>>, Error> {
        let memory = self.memory(pool)?;

        let mut page = None;
        if self.count == self.size {
            let mut pool = pool.lock();
            let layout = Layout::from_size_align(self.obj_size, mem::align_of::<LAddr>()).unwrap();

            self.count -= 1;

            let addr = unsafe { memory.as_ptr().add(self.count).read() };

            page = pool.deallocate(addr, layout).unwrap_or(None);
        }

        if self.count < self.size {
            unsafe { memory.as_ptr().add(self.count).write(addr) };
            self.count += 1;
            Ok(page)
        } else {
            Err(Error::Internal("No more space for deallocating the object"))
        }
    }
}
