use core::{alloc::Layout, ptr::NonNull};

use bootfs::parse::Directory;
use cstr_core::CStr;
use object::{
    elf::{PF_R, PF_W, PF_X, PT_GNU_STACK, PT_INTERP, PT_LOAD},
    read::{
        self,
        elf::{ElfFile64, ProgramHeader},
    },
    Endianness, Object, ObjectKind,
};
use solvent::prelude::{Flags, Phys, PhysRef, Space, PAGE_LAYOUT, PAGE_MASK, PAGE_SIZE};
use sv_call::task::DEFAULT_STACK_SIZE;

const STACK_PROTECTOR_SIZE: usize = PAGE_SIZE;

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

fn flags(seg_flags: u32) -> Flags {
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
}

fn load_seg(
    image: &PhysRef,
    space: &Space,
    e: Endianness,
    seg: &impl ProgramHeader<Endian = object::Endianness>,
    kind: ObjectKind,
) -> Result<usize, Error> {
    let msize = seg.p_memsz(e).into() as usize;
    let fsize = seg.p_filesz(e).into() as usize;
    let offset = seg.p_offset(e).into() as usize;
    let address = seg.p_vaddr(e).into() as usize;

    if offset & PAGE_MASK != address & PAGE_MASK {
        return Err(Error::Solvent(solvent::error::Error::EALIGN));
    }
    let fend = (offset + fsize) & !PAGE_MASK;
    let cend = offset + fsize;
    let mend = (offset + msize).next_multiple_of(PAGE_SIZE);
    let offset = offset & !PAGE_MASK;
    let address = address & !PAGE_MASK;
    let fsize = fend - offset;
    let csize = cend - fend;
    let asize = mend.saturating_sub(fend);

    let data_phys = image
        .dup_sub(offset, fsize)
        .ok_or(Error::Solvent(solvent::error::Error::ERANGE))?;

    let flags = flags(seg.p_flags(e));

    let base = match kind {
        ObjectKind::Dynamic if fsize == 0 => None,
        ObjectKind::Dynamic => Some(space.map_ref(None, data_phys, flags)?.as_mut_ptr() as usize),
        _ if fsize == 0 => Some(address),
        _ => Some(space.map_ref(Some(address), data_phys, flags)?.as_mut_ptr() as usize),
    };

    let base = if asize > 0 {
        let address = base.map(|base| base + fsize);

        let layout =
            Layout::from_size_align(asize, PAGE_SIZE).map_err(solvent::error::Error::from)?;
        let mem = Phys::allocate(layout, flags | Flags::WRITABLE | Flags::ZEROED)?;

        let cdata = image.read(fend, csize)?;
        unsafe { mem.write(0, &cdata) }?;

        let abase = space.map(address, mem, 0, asize, flags)?.as_mut_ptr() as usize;
        Some(abase - fsize)
    } else {
        base
    };
    Ok(base.expect("Null segment"))
}

fn load_segs<'a>(
    mut image: Image<'a>,
    bootfs: Directory<'a>,
    bootfs_phys: &PhysRef,
    space: &Space,
) -> Result<(NonNull<u8>, usize, Flags), Error> {
    let file = loop {
        let file = ElfFile64::<'a, Endianness, _>::parse(image.data)?;

        match file
            .raw_segments()
            .iter()
            .find(|seg| seg.p_type(file.endian()) == PT_INTERP)
        {
            Some(interp) => {
                use solvent::error::Error as SvError;
                let interp = CStr::from_bytes_with_nul(
                    interp
                        .data(file.endian(), file.data())
                        .map_err(|_| SvError::EBUFFER)?,
                )
                .map_err(|_| SvError::EBUFFER)?;

                let data = bootfs
                    .find(interp.to_bytes(), b'/')
                    .ok_or(SvError::ENOENT)
                    .inspect_err(|_| {
                        log::error!("Failed to find the interpreter for the executable")
                    })?;

                let phys = crate::sub_phys(data, bootfs, bootfs_phys)?;
                image = unsafe { Image::new(data, phys).ok_or(SvError::ENOENT) }?;
            }
            None => break file,
        }
    };

    let mut entry = file.entry() as usize;
    let kind = file.kind();

    let (mut stack_size, mut stack_flags) = (
        DEFAULT_STACK_SIZE,
        Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS,
    );
    for seg in file.raw_segments() {
        match seg.p_type(file.endian()) {
            PT_LOAD if seg.p_memsz(file.endian()) > 0 => {
                let base = load_seg(&image.phys, space, file.endian(), seg, kind)?;
                let fbase = seg.p_offset(file.endian()) as usize;
                let fend = fbase + seg.p_filesz(file.endian()) as usize;
                if kind == ObjectKind::Dynamic && (fbase..fend).contains(&entry) {
                    entry = entry + base - fbase;
                }
            }
            PT_GNU_STACK => {
                let ss = seg.p_memsz(file.endian()) as usize;
                if ss > 0 {
                    stack_size = ss;
                }
                stack_flags = flags(seg.p_flags(file.endian()));
            }
            _ => {}
        }
    }

    let entry = NonNull::new(entry as *mut u8).ok_or(solvent::error::Error::EINVAL)?;
    Ok((entry, stack_size, stack_flags))
}

pub fn load_elf(
    image: Image,
    bootfs: Directory,
    bootfs_phys: &PhysRef,
    space: &Space,
) -> Result<(NonNull<u8>, NonNull<u8>), Error> {
    let (entry, stack_size, stack_flags) = load_segs(image, bootfs, bootfs_phys, space)?;

    let stack = {
        let stack_layout =
            Layout::from_size_align(stack_size + STACK_PROTECTOR_SIZE * 2, PAGE_LAYOUT.align())
                .map_err(solvent::error::Error::from)?;
        let stack_phys = Phys::allocate(stack_layout, stack_flags)?;
        let stack_ptr = space.map(None, stack_phys, 0, stack_layout.size(), stack_flags)?;

        let base = stack_ptr.as_non_null_ptr();
        let actual_end =
            unsafe { NonNull::new_unchecked(base.as_ptr().add(stack_size + STACK_PROTECTOR_SIZE)) };

        let prefix = NonNull::slice_from_raw_parts(base, STACK_PROTECTOR_SIZE);
        let suffix = NonNull::slice_from_raw_parts(actual_end, STACK_PROTECTOR_SIZE);
        unsafe { space.reprotect(prefix, Flags::USER_ACCESS) }?;
        unsafe { space.reprotect(suffix, Flags::USER_ACCESS) }?;

        actual_end
    };

    Ok((entry, stack))
}
