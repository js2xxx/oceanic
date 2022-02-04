bitflags::bitflags! {
    /// Flags to describe a block of memory.
    pub struct Flags: u32 {
        const USER_ACCESS = 1;
        const READABLE    = 1 << 1;
        const WRITABLE    = 1 << 2;
        const EXECUTABLE  = 1 << 3;
        const UNCACHED    = 1 << 4;
        const ZEROED      = 1 << 10;
    }
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct MemInfo {
    pub all_available: usize,
    pub current_used: usize,
}

cfg_if::cfg_if! { if #[cfg(target_arch = "x86_64")] {

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 4096;

} }

#[repr(C)]
pub struct MapInfo {
    pub addr: usize,
    pub map_addr: bool,
    pub phys: crate::Handle,
    pub phys_offset: usize,
    pub len: usize,
    pub flags: u32,
}

#[cfg(feature = "call")]
pub fn test() {
    let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
    let phys = crate::call::phys_alloc(4096, 4096, flags.bits)
        .expect("Failed to allocate physical object");
    let mi = MapInfo {
        addr: 0,
        map_addr: false,
        phys,
        phys_offset: 0,
        len: 4096,
        flags: flags.bits,
    };
    let ptr =
        crate::call::mem_map(crate::Handle::NULL, &mi).expect("Failed to map the physical memory");
    unsafe { ptr.cast::<u32>().write(12345) };
    crate::call::mem_unmap(crate::Handle::NULL, ptr).expect("Failed to unmap the memory");
    crate::call::obj_drop(phys).expect("Failed to deallocate the physical object");
}
