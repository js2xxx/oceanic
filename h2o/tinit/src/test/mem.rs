use solvent::prelude::{Flags, Phys, Virt, PAGE_LAYOUT, PAGE_SIZE};

pub unsafe fn test(virt: &Virt) {
    let sub = virt
        .allocate(None, PAGE_LAYOUT)
        .expect("Failed to allocate sub-virt");
    let phys = Phys::allocate(PAGE_SIZE, true).expect("Failed to allocate memory");
    let ptr = sub
        .map(
            None,
            phys.clone(),
            0,
            PAGE_LAYOUT,
            Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS,
        )
        .expect("Failed to map memory");
    unsafe { ptr.as_mut_ptr().write(0x64) };
    sub.destroy().expect("Failed to destroy sub-virt");
    let buf = phys.read(0, 1).expect("Failed to read memory");
    assert_eq!(&buf, &[0x64]);
}
