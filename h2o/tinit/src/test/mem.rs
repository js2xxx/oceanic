use sv_call::{
    mem::{Flags, MapInfo},
    *,
};

pub unsafe fn test() {
    let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
    let phys = sv_phys_alloc(4096, 4096, flags)
        .into_res()
        .expect("Failed to allocate physical object");

    let mi = MapInfo {
        addr: 0,
        map_addr: false,
        phys,
        phys_offset: 0,
        len: 4096,
        flags,
    };

    let ptr = sv_mem_map(Handle::NULL, &mi)
        .into_res()
        .expect("Failed to map the physical memory") as *mut u8;

    let data = [1, 2, 3, 4];
    unsafe { ptr.copy_from_nonoverlapping(data.as_ptr(), data.len()) };

    sv_mem_unmap(Handle::NULL, ptr)
        .into_res()
        .expect("Failed to unmap the memory");

    let mut buf = [0; 4];
    sv_phys_read(phys, 0, buf.len(), buf.as_mut_ptr())
        .into_res()
        .expect("Failed to read from physical memory");
    assert_eq!(buf, data);

    sv_obj_drop(phys)
        .into_res()
        .expect("Failed to deallocate the physical object");
}
