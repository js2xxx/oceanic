use alloc::string::String;

use bitop_ex::BitOpEx;
use goblin::elf::*;
use paging::{LAddr, PAddr};

use super::*;
use crate::{
    cpu::CpuMask,
    mem::space::{Flags, Phys, Space},
};

fn load_prog(
    space: &Arc<Space>,
    flags: u32,
    virt: LAddr,
    phys: PAddr,
    fsize: usize,
    msize: usize,
) -> solvent::Result {
    fn flags_to_pg_attr(flags: u32) -> Flags {
        let mut ret = Flags::USER_ACCESS;
        if (flags & program_header::PF_R) != 0 {
            ret |= Flags::READABLE;
        }
        if (flags & program_header::PF_W) != 0 {
            ret |= Flags::WRITABLE;
        }
        if (flags & program_header::PF_X) != 0 {
            ret |= Flags::EXECUTABLE;
        }
        ret
    }
    log::trace!(
        "Loading LOAD phdr (flags = {:?}, virt = {:?}, phys = {:?}, fsize = {:#x}, msize = {:#x})",
        flags,
        virt,
        phys,
        fsize,
        msize
    );

    let flags = flags_to_pg_attr(flags);
    let (vstart, vend) = (virt.val(), virt.val() + fsize);

    if fsize > 0 {
        let virt = LAddr::from(vstart)..LAddr::from(vend);
        log::trace!("Mapping {:?}", virt);
        let cnt = fsize.div_ceil_bit(paging::PAGE_SHIFT);
        let (layout, _) = paging::PAGE_LAYOUT.repeat(cnt)?;
        let phys = Phys::new(phys, layout, flags);
        unsafe { space.map_addr(virt, Some(phys), flags) }?;
    }

    if msize > fsize {
        let extra = msize - fsize;

        let virt = LAddr::from(vend)..LAddr::from(vend + extra);
        log::trace!("Allocating {:?}", virt);
        unsafe { space.map_addr(virt, None, flags | Flags::ZEROED) }?;
    }

    Ok(())
}

fn load_elf(space: &Arc<Space>, file: &Elf, image: &[u8]) -> solvent::Result<(LAddr, usize)> {
    log::trace!(
        "Loading ELF file from image {:?}, space = {:?}",
        image.as_ptr(),
        space as *const _
    );
    let entry = LAddr::new(file.entry as *mut u8);
    let mut stack_size = DEFAULT_STACK_SIZE;

    for phdr in file.program_headers.iter() {
        match phdr.p_type {
            program_header::PT_GNU_STACK => {
                if phdr.p_memsz != 0 {
                    log::trace!("Found stack size {:?}", phdr.p_memsz);
                    stack_size = phdr.p_memsz as usize
                }
            }

            program_header::PT_GNU_RELRO => {}

            program_header::PT_LOAD => load_prog(
                space,
                phdr.p_flags,
                LAddr::from(phdr.p_vaddr as usize),
                LAddr::new(unsafe { image.as_ptr().add(phdr.p_offset as usize) } as *mut u8)
                    .to_paddr(minfo::ID_OFFSET),
                (phdr.p_filesz as usize).round_up_bit(paging::PAGE_SHIFT),
                (phdr.p_memsz as usize).round_up_bit(paging::PAGE_SHIFT),
            )?,

            _ => return Err(solvent::Error::ESPRT),
        }
    }
    Ok((entry, stack_size))
}

pub fn from_elf(
    image: &[u8],
    name: String,
    affinity: CpuMask,
    init_chan: Ref<dyn Any>,
) -> solvent::Result<(Init, Handle)> {
    let file = Elf::parse(image)
        .map_err(|_| solvent::Error::EINVAL)
        .and_then(|file| {
            if file.is_64 {
                Ok(file)
            } else {
                Err(solvent::Error::EPERM)
            }
        })?;

    let tid = crate::sched::SCHED.with_current(|cur| Ok(cur.tid.clone()))?;
    let space = Space::new(Type::User);
    let (entry, stack_size) = load_elf(&space, &file, image)?;

    super::create_inner(
        tid,
        Some(name),
        Some(Type::User),
        Some(affinity),
        space,
        entry,
        init_chan,
        0,
        stack_size,
    )
}
