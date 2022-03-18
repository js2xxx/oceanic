use core::alloc::Layout;

use bootfs::parse::Directory;
use object::{
    elf::{PF_R, PF_W, PF_X, PT_GNU_STACK, PT_INTERP, PT_LOAD},
    read::{
        self,
        elf::{ElfFile64, ProgramHeader},
    },
    Endianness, Object, ObjectKind,
};
use solvent::prelude::{Flags, Phys, PhysRef, Space, PAGE_MASK, PAGE_SIZE};
use sv_call::task::DEFAULT_STACK_SIZE;

use crate::c_str::CStr;

pub struct Image<'a> {
    data: &'a [u8],
    phys: PhysRef,
}

impl<'a> Image<'a> {
    /// # Safety
    ///
    /// `data` must be the mapped memory location corresponding to `phys`.
    pub unsafe fn new(data: &'a [u8], phys: PhysRef) -> Option<Self> {
        (data.len() == phys.len()).then(|| Image { data, phys })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Parse(read::Error),
    Solvent(solvent::error::Error),
}

impl From<read::Error> for Error {
    fn from(err: read::Error) -> Self {
        Error::Parse(err)
    }
}

impl From<solvent::error::Error> for Error {
    fn from(err: solvent::error::Error) -> Self {
        Error::Solvent(err)
    }
}

fn load_phdr(
    image: &Image,
    space: &Space,
    end: Endianness,
    seg: &impl ProgramHeader<Endian = object::Endianness>,
    kind: ObjectKind,
) -> Result<usize, Error> {
    let msize = seg.p_memsz(end).into() as usize;
    let fsize = seg.p_filesz(end).into() as usize;
    let offset = seg.p_offset(end).into() as usize;
    let address = seg.p_vaddr(end).into() as usize;

    if offset & PAGE_MASK != address & PAGE_MASK {
        return Err(Error::Solvent(solvent::error::Error::EALIGN));
    }
    let fend = (offset + fsize).next_multiple_of(PAGE_SIZE);
    let mend = (offset + msize).next_multiple_of(PAGE_SIZE);
    let offset = offset & !PAGE_MASK;
    let address = address & !PAGE_MASK;
    let fsize = fend - offset;
    let msize = mend - offset;

    let data_phys = image
        .phys
        .dup_sub(offset, fsize)
        .ok_or(Error::Solvent(solvent::error::Error::ERANGE))?;

    let flags = {
        let seg_flags = seg.p_flags(end);
        let mut flags = Flags::USER_ACCESS;
        if seg_flags & PF_R != 0 {
            flags |= Flags::READABLE;
        }
        if seg_flags & PF_W != 0 {
            flags |= Flags::WRITABLE;
        }
        if seg_flags & PF_X != 0 {
            flags |= Flags::EXECUTABLE;
        }
        flags
    };

    let address = match kind {
        ObjectKind::Dynamic => None,
        _ => Some(address),
    };
    let base = space.map_ref(address, data_phys, flags)?.as_mut_ptr() as usize;

    if msize > fsize {
        let address = base + fsize;
        let len = msize - fsize;

        let layout =
            Layout::from_size_align(len, PAGE_SIZE).map_err(solvent::error::Error::from)?;
        let mem = Phys::allocate(layout, flags)?;
        space.map(Some(address), mem, 0, len, flags)?;
    }
    Ok(base)
}

pub fn load_elf(
    image: Image,
    bootfs: Directory,
    bootfs_phys: &PhysRef,
    space: &Space,
) -> Result<(usize, usize), Error> {
    let file = ElfFile64::<'_, Endianness, _>::parse(image.data)?;

    let mut entry = file.entry() as usize;
    let kind = file.kind();

    let mut stack_size = DEFAULT_STACK_SIZE;
    let mut interp = None;
    for seg in file.raw_segments() {
        match seg.p_type(file.endian()) {
            PT_LOAD => {
                let base = load_phdr(&image, space, file.endian(), seg, kind)?;
                let fbase = seg.p_offset(file.endian()) as usize;
                let fend = fbase + seg.p_filesz(file.endian()) as usize;
                if kind == ObjectKind::Dynamic && (fbase..fend).contains(&entry) {
                    let voff = seg.p_vaddr(file.endian()) as usize - fbase;
                    entry = entry + base - voff;
                }
            }
            PT_GNU_STACK => {
                let ss = seg.p_memsz(file.endian()) as usize;
                if ss > 0 {
                    stack_size = ss;
                }
            }
            PT_INTERP => {
                interp = Some(
                    CStr::from_bytes_with_nul(
                        seg.data(file.endian(), file.data())
                            .expect("Index out of bounds"),
                    )
                    .expect("Not a valid cstring"),
                )
            }
            _ => {}
        }
    }

    if let Some(interp) = interp {
        use solvent::error::Error;
        let data = bootfs
            .find(interp.to_bytes(), b'/')
            .ok_or(Error::ENOENT)
            .inspect_err(|_| log::error!("Failed to find the interpreter for the executable"))?;

        let phys = crate::sub_phys(data, bootfs, bootfs_phys)?;
        (entry, _) = load_elf(
            unsafe { Image::new(data, phys).ok_or(Error::ENOENT) }?,
            bootfs,
            bootfs_phys,
            space,
        )?;
    }

    Ok((entry, stack_size))
}
