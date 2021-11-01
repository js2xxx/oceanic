use crate::{PAddr, PAGE_SIZE};

/// # Safety
///
/// The page allocator is responsible for maintaining the infrastructure of the
/// system.
pub unsafe trait PageAlloc {
    /// # Safety
    ///
    /// This function may directly call the allocator unlockedly.
    unsafe fn alloc(&mut self) -> Option<PAddr>;

    /// # Safety
    ///
    /// This function may directly call the allocator unlockedly.
    unsafe fn dealloc(&mut self, addr: PAddr);

    /// # Safety
    ///
    /// This function may directly call the allocator unlockedly.
    unsafe fn alloc_zeroed(&mut self, id_off: usize) -> Option<PAddr> {
        let phys = self.alloc()?;
        let virt = phys.to_laddr(id_off);

        let page = core::slice::from_raw_parts_mut(*virt, PAGE_SIZE);
        page.fill(0);

        Some(phys)
    }
}
