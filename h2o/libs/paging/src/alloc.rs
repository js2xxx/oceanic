use crate::{PAddr, PAGE_SIZE};

pub trait PageAlloc {
      fn alloc(&mut self) -> Option<PAddr>;
      fn dealloc(&mut self, addr: PAddr);

      fn alloc_zeroed(&mut self, id_off: usize) -> Option<PAddr> {
            let phys = self.alloc()?;
            let virt = phys.get() + id_off;

            let page = unsafe { core::slice::from_raw_parts_mut(virt as *mut u8, PAGE_SIZE)};
            page.copy_from_slice(&[0; PAGE_SIZE]);
            
            Some(phys)
      }
}