use alloc::string::String;
use core::ops::Range;

use bitop_ex::BitOpEx;
use goblin::elf::*;
use paging::{LAddr, PAddr};

use super::*;
use crate::{
    cpu::CpuMask,
    mem::space::{self, Flags, Phys, Space, Virt},
};

fn map_addr(
    virt: &Arc<Virt>,
    addr: Range<LAddr>,
    phys: Option<Phys>,
    flags: Flags,
) -> sv_call::Result {
    let offset = addr
        .start
        .val()
        .checked_sub(virt.range().start.val())
        .ok_or(sv_call::ERANGE)?;
    let len = addr
        .end
        .val()
        .checked_sub(addr.start.val())
        .ok_or(sv_call::ERANGE)?;
    let phys = match phys {
        Some(phys) => phys,
        None => Phys::allocate(len, true, false)?,
    };
    virt.map(Some(offset), phys, 0, space::page_aligned(len), flags)?;
    Ok(())
}

fn load_prog(
    space: &Arc<Space>,
    flags: u32,
    virt: LAddr,
    phys: PAddr,
    fsize: usize,
    msize: usize,
) -> sv_call::Result {
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
    let fend = fsize.round_down_bit(paging::PAGE_SHIFT);
    let cstart = fend;
    let cend = fsize;
    let mstart = fend;
    let mend = msize;

    let flags = flags_to_pg_attr(flags);
    let (vstart, vend) = (virt.val(), virt.val() + fend);

    if fend > 0 {
        let virt = LAddr::from(vstart)..LAddr::from(vend);
        log::trace!("Mapping {:?}", virt);
        let phys = Phys::new(phys, fend)?;
        map_addr(space, virt, Some(phys), flags)?;
    }

    if mend > mstart {
        let extra = mend - mstart;

        let virt = LAddr::from(vend)..LAddr::from(vend + extra);
        log::trace!("Allocating {:?}", virt);
        map_addr(space, virt.clone(), None, flags)?;

        if cend > cstart {
            unsafe {
                let dst = virt.start;
                let src = phys.to_laddr(minfo::ID_OFFSET).add(cstart);
                let csize = cend - cstart;
                log::trace!("Copying {:?}", dst..LAddr::from(dst.val() + csize));

                dst.copy_from_nonoverlapping(src, csize);
            }
        }
    }

    Ok(())
}

fn load_elf(space: &Arc<Space>, file: &Elf, image: &[u8]) -> sv_call::Result<(LAddr, usize)> {
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

            _ => return Err(sv_call::ESPRT),
        }
    }
    Ok((entry, stack_size))
}

pub fn from_elf(
    image: &[u8],
    space: Arc<super::Space>,
    name: String,
    affinity: CpuMask,
    init_chan: hdl::Ref,
) -> sv_call::Result<Init> {
    let file = Elf::parse(image)
        .map_err(|_| sv_call::EINVAL)
        .and_then(|file| {
            if file.is_64 {
                Ok(file)
            } else {
                Err(sv_call::EPERM)
            }
        })?;

    let init_chan = space.handles().insert_ref(init_chan)?;

    let (entry, stack_size) = load_elf(space.mem(), &file, image)?;
    let stack = space::init_stack(space.mem(), stack_size)?;

    let starter = super::Starter {
        entry,
        stack,
        arg: 0,
    };

    let tid = crate::sched::SCHED.with_current(|cur| Ok(cur.tid.clone()))?;

    let ret = super::exec_inner(
        tid,
        Some(name),
        Some(Type::User),
        Some(affinity),
        space,
        init_chan,
        &starter,
    )?;

    crate::sched::SCHED.with_current(|cur| {
        let event = Arc::downgrade(&ret.tid().event) as _;
        cur.space().handles().insert(ret.tid().clone(), Some(event))
    })?;
    Ok(ret)
}
