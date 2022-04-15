#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::{mem, ops::Range};

use goblin::elf64::{header::*, program_header::*, section_header::*};
use solvent::prelude::{Flags, Phys, Virt, PAGE_MASK};

#[derive(Debug)]
pub enum Error {
    ElfParse(goblin::error::Error),
    NotSupported(&'static str),
    PhysAlloc(solvent::error::Error),
    PhysRead(solvent::error::Error),
    PhysWrite(solvent::error::Error),
    PhysSub(solvent::error::Error),
    VirtAlloc(solvent::error::Error),
    VirtMap(solvent::error::Error),
}

pub struct LoadedElf {
    pub is_dyn: bool,
    pub virt: Virt,
    pub range: Range<usize>,
    /// Note: The size of the stack can be zero and the caller should check it before allocating
    /// memory for the stack.
    pub stack: Option<(usize, Flags)>,
    pub entry: usize,
    pub dynamic: Option<ProgramHeader>,
    pub tls: Option<ProgramHeader>,
    pub sym_len: usize,
}

pub fn parse_flags(flags: u32) -> Flags {
    let mut ret = Flags::USER_ACCESS;
    if flags & PF_R != 0 {
        ret |= Flags::READABLE;
    }
    if flags & PF_W != 0 {
        ret |= Flags::WRITABLE;
    }
    if flags & PF_X != 0 {
        ret |= Flags::EXECUTABLE;
    }
    ret
}

pub fn parse_header(phys: &Phys, dyn_only: bool) -> Result<(Header, bool), Error> {
    let data = phys
        .read(0, mem::size_of::<Header>())
        .map_err(Error::PhysRead)?;

    let header = Header::parse(&data).map_err(Error::ElfParse)?;

    if header.e_ident[EI_CLASS] != ELFCLASS64 {
        return Err(Error::NotSupported("Only support 64-bit file"));
    }
    if header.e_ident[EI_DATA] != ELFDATA2LSB {
        return Err(Error::NotSupported("Only support little endian file"));
    }
    if (dyn_only || header.e_type != ET_EXEC) && header.e_type != ET_DYN {
        return Err(Error::NotSupported(
            "Only support dynamic (or executable if enabled) file",
        ));
    }

    Ok((header, header.e_type == ET_DYN))
}

pub fn parse_segments(
    phys: &Phys,
    offset: usize,
    count: usize,
) -> Result<Vec<ProgramHeader>, Error> {
    let data = phys
        .read(offset, count * mem::size_of::<ProgramHeader>())
        .map_err(Error::PhysRead)?;

    Ok(ProgramHeader::from_bytes(&data, count))
}

pub fn parse_sections(
    phys: &Phys,
    offset: usize,
    count: usize,
) -> Result<Vec<SectionHeader>, Error> {
    let data = phys
        .read(offset, count * mem::size_of::<SectionHeader>())
        .map_err(Error::PhysRead)?;

    Ok(SectionHeader::from_bytes(&data, count))
}

pub fn get_addr_range_info(segments: &[ProgramHeader]) -> (usize, usize) {
    segments
        .iter()
        .filter(|segment| segment.p_type == PT_LOAD)
        .fold((usize::MAX, 0), |(min, max), segment| {
            let base = segment.p_vaddr as usize;
            let size = segment.p_memsz as usize;
            (min.min(base), max.max(base + size))
        })
}

pub fn map_segment(
    segment: &ProgramHeader,
    phys: &Phys,
    virt: &Virt,
    base_offset: usize,
) -> Result<(), Error> {
    let msize = segment.p_memsz as usize;
    let fsize = segment.p_filesz as usize;
    let offset = segment.p_offset as usize;
    let address = segment.p_vaddr as usize;

    if offset & PAGE_MASK != address & PAGE_MASK {
        return Err(Error::NotSupported(
            "Offset of segments must be page aligned",
        ));
    }
    let fend = (offset + fsize) & !PAGE_MASK;
    let cend = offset + fsize;
    let mend = (offset + msize + PAGE_MASK) & !PAGE_MASK;
    let offset = offset & !PAGE_MASK;
    let address = address & !PAGE_MASK;
    let fsize = fend - offset;
    let csize = cend - fend;
    let asize = mend.saturating_sub(fend);

    let flags = parse_flags(segment.p_flags);

    if fsize > 0 {
        let data = phys
            .create_sub(offset, fsize, true)
            .map_err(Error::PhysSub)?;

        log::trace!(
            "Map {:#x}~{:#x} -> {:#x}",
            offset,
            offset + fsize,
            address - base_offset
        );
        virt.map_phys(Some(address - base_offset), data, flags)
            .map_err(Error::VirtMap)?;
    }

    if asize > 0 {
        let address = address + fsize;

        let mem = Phys::allocate(asize, true).map_err(Error::PhysAlloc)?;

        let cdata = phys.read(fend, csize).map_err(Error::PhysRead)?;
        unsafe { mem.write(0, &cdata) }.map_err(Error::PhysWrite)?;

        log::trace!(
            "Alloc {:#x}~{:#x} -> {:#x}",
            fend,
            fend + asize,
            address - base_offset
        );
        virt.map_phys(Some(address - base_offset), mem, flags)
            .map_err(Error::VirtMap)?;
    }
    Ok(())
}

pub fn get_interp(phys: &Phys) -> Result<Option<Vec<u8>>, Error> {
    let (header, _) = parse_header(phys, true)?;
    let segments = parse_segments(phys, header.e_phoff as usize, header.e_phnum as usize)?;
    { segments.iter() }
        .find_map(|segment| {
            (segment.p_type == PT_INTERP).then(|| {
                let offset = segment.p_offset as usize;
                let size = segment.p_filesz as usize;

                phys.read(offset, size).map_err(Error::PhysRead)
            })
        })
        .transpose()
}

pub fn load(phys: &Phys, dyn_only: bool, root_virt: &Virt) -> Result<LoadedElf, Error> {
    let (header, is_dyn) = parse_header(phys, dyn_only)?;

    let segments = parse_segments(phys, header.e_phoff as usize, header.e_phnum as usize)?;
    let sections = parse_sections(phys, header.e_shoff as usize, header.e_shnum as usize)?;
    let (min, max) = get_addr_range_info(&segments);

    let virt = {
        let layout = unsafe { Virt::page_aligned(max - min) };
        if is_dyn {
            root_virt.allocate(None, layout).map_err(Error::VirtAlloc)?
        } else {
            let base = root_virt.base().as_ptr() as usize;
            let offset = min.checked_sub(base).ok_or(Error::NotSupported(
                "The specified address is out of bounds",
            ))?;
            root_virt
                .allocate(Some(offset), layout)
                .map_err(Error::VirtAlloc)?
        }
    };

    struct Guard<'a>(&'a Virt);
    impl<'a> Drop for Guard<'a> {
        fn drop(&mut self) {
            let _ = self.0.destroy();
        }
    }
    let guard = Guard(&virt);

    let base = virt.base().as_ptr() as usize;
    let base_offset = if is_dyn { 0 } else { base };
    let entry = header.e_entry as usize + base - base_offset;

    let mut stack = None;
    let mut dynamic = None;
    let mut tls = None;
    for segment in segments {
        match segment.p_type {
            PT_LOAD => map_segment(&segment, phys, &virt, base_offset)?,
            PT_GNU_STACK => stack = Some((segment.p_memsz as usize, parse_flags(segment.p_flags))),
            PT_DYNAMIC => dynamic = Some(segment),
            PT_TLS => tls = Some(segment),
            _ => {}
        }
    }

    let sym_len = sections
        .into_iter()
        .find_map(|section| {
            (section.sh_type == SHT_DYNSYM).then(|| (section.sh_size / section.sh_entsize) as usize)
        })
        .unwrap_or_default();

    mem::forget(guard);
    Ok(LoadedElf {
        is_dyn,
        virt,
        range: base..(base + max - min),
        stack,
        entry,
        dynamic,
        tls,
        sym_len,
    })
}
