use core::{alloc::Layout, arch::asm, mem::size_of};

use bitop_ex::BitOpEx;
use goblin::elf::*;
use uefi::prelude::*;

/// Transform the flags of a ELF program header into the attribute of a paging
/// entry.
///
/// In this case, we only focus on the read/write-ability and executability.
fn flags_to_pg_attr(flags: u32) -> paging::Attr {
    let mut ret = paging::Attr::PRESENT;
    if (flags & program_header::PF_W) != 0 {
        ret |= paging::Attr::WRITABLE;
    }
    if (flags & program_header::PF_X) == 0 {
        ret |= paging::Attr::EXE_DISABLE;
    }
    ret
}

/// Load a loadable ELF program header.
///
/// The program segment mapping is like the graph below:
///
///       |<----File size: Directly mapping----->|<-Extra: Allocation &
/// Mapping->|       |<----------------------Memory
/// size----------------------------------->|
///
/// # Arguments
/// * `virt` - The base linear address where the segment should be loaded.
/// * `phys` - The base physical address where the segment is located.
/// * `fsize` - The size of the file stored in the media.
/// * `msize` - The size of the program required in the memory.
fn load_prog(
    syst: &SystemTable<Boot>,
    flags: u32,
    virt: paging::LAddr,
    phys: paging::PAddr,
    fsize: usize,
    msize: usize,
) {
    log::trace!(
        "file::load_prog: flags = {:?}, virt = {:?}, phys = {:?}, fsize = {:x}, msize = {:x}",
        flags,
        virt,
        phys,
        fsize,
        msize
    );

    let pg_attr = flags_to_pg_attr(flags);
    let (vstart, vend) = (virt.val(), virt.val() + fsize);

    if fsize > 0 {
        let virt = paging::LAddr::from(vstart)..paging::LAddr::from(vend);
        crate::mem::maps(syst, virt, phys, pg_attr).expect("Failed to map virtual memory");
    }

    if msize > fsize {
        let extra = msize - fsize;
        let phys = crate::mem::alloc(syst)
            .alloc_n(extra >> paging::PAGE_SHIFT)
            .expect("Failed to allocate extra memory");
        // Must clear the memory, otherwise some static variables will be uninitialized.
        unsafe { core::ptr::write_bytes(*phys.to_laddr(crate::mem::EFI_ID_OFFSET), 0, extra) };

        let virt = paging::LAddr::from(vend)..paging::LAddr::from(vend + extra);
        crate::mem::maps(syst, virt, phys, pg_attr).expect("Failed to map virtual memory");
    }
}

/// Load a Processor-Local Storage (PLS) segment.
fn load_pls(syst: &SystemTable<Boot>, size: usize, align: usize) -> Layout {
    log::trace!("file::map: loading TLS: size = {:?}", size);

    let layout = Layout::from_size_align(size, align)
        .expect("Failed to create the PLS layout")
        .pad_to_align();
    let size = layout.size();

    let pls = {
        let alloc_size = size + size_of::<*mut usize>();
        let laddr = crate::mem::alloc(syst)
            .alloc_n(alloc_size.div_ceil_bit(paging::PAGE_SHIFT))
            .expect("Failed to allocate memory for PLS")
            .to_laddr(crate::mem::EFI_ID_OFFSET);
        *laddr
    };

    unsafe {
        let self_ptr = pls.add(size).cast::<usize>();
        // TLS's self-pointer is written its physical address there,
        // and therefore should be modified in the kernel.
        self_ptr.write(self_ptr as usize);

        const FS_BASE: u64 = 0xC0000100;
        asm!(
              "wrmsr",
              in("ecx") FS_BASE,
              in("eax") self_ptr,
              in("edx") self_ptr as u64 >> 32,
              options(nostack)
        );
    }

    layout
}

/// Map a ELF executable into the memory.
///
/// # Returns
///
/// This function returns a tuple with 2 elements where the first element is the
/// entry point of the ELF executable and the second element is the TLS size of
/// it.
pub fn map_elf(syst: &SystemTable<Boot>, data: &[u8]) -> (*mut u8, Option<Layout>) {
    log::trace!(
        "file::map: syst = {:?}, data = {:?}",
        syst as *const _,
        data.as_ptr()
    );

    let file = Elf::parse(data).expect("Failed to parse ELF64 file");
    assert!(file.is_64);

    let mut pls_layout = None;
    for phdr in file.program_headers.iter() {
        match phdr.p_type {
            program_header::PT_LOAD => load_prog(
                syst,
                phdr.p_flags,
                paging::LAddr::from(phdr.p_vaddr as usize),
                paging::PAddr::new(unsafe { data.as_ptr().add(phdr.p_offset as usize) } as usize),
                (phdr.p_filesz as usize).round_up_bit(paging::PAGE_SHIFT),
                (phdr.p_memsz as usize).round_up_bit(paging::PAGE_SHIFT),
            ),

            program_header::PT_TLS => {
                pls_layout = Some(load_pls(syst, phdr.p_memsz as usize, phdr.p_align as usize));
            }

            _ => {}
        }
    }

    let entry = paging::LAddr::from(file.entry as usize);
    (*entry, pls_layout)
}
