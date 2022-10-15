use solvent::prelude::{Flags, Phys, PhysOptions, Virt, PAGE_LAYOUT, PAGE_SIZE};

pub unsafe fn test(virt: &Virt) {
    let sub = virt
        .allocate(None, PAGE_LAYOUT)
        .expect("Failed to allocate sub-virt");
    let phys = Phys::allocate(PAGE_SIZE, PhysOptions::ZEROED).expect("Failed to allocate memory");
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

    let phys = Phys::allocate(10, PhysOptions::ZEROED | PhysOptions::RESIZABLE)
        .expect("Failed to allocate memory");
    unsafe { phys.write(2, &[1, 2, 3]) }.expect("Failed to write to phys");
    phys.resize(4, true).expect("Failed to resize the phys");
    let buf = phys.read(1, 10).expect("Failed to read from phys");
    assert_eq!(&buf, &[0, 1, 2]);
}
