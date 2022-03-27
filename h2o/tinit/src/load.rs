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
use solvent::prelude::{Flags, Phys, Virt, PAGE_MASK, PAGE_SIZE};
use sv_call::task::DEFAULT_STACK_SIZE;

const STACK_PROTECTOR_SIZE: usize = PAGE_SIZE;

#[derive(Clone)]
pub struct Image<'a> {
    pub data: &'a [u8],
    pub phys: Phys,
}

impl<'a> Image<'a> {
    /// # Safety
    ///
    /// `data` must be the mapped memory location corresponding to `phys`.
    pub unsafe fn new(data: &'a [u8], phys: Phys) -> Option<Self> {
        (data.len().next_multiple_of(PAGE_SIZE) == phys.len()).then(|| Image { data, phys })
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
    image: &Phys,
    virt: &Virt,
    e: Endianness,
    seg: &impl ProgramHeader<Endian = object::Endianness>,
    base: usize,
) -> Result<(), Error> {
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

    let flags = flags(seg.p_flags(e));

    if fsize > 0 {
        let data = image.create_sub(offset, fsize, true)?;

        log::trace!(
            "Map {:#x}~{:#x} -> {:#x}",
            offset,
            offset + fsize,
            address - base
        );
        virt.map_phys(Some(address - base), data, flags)?;
    }

    if asize > 0 {
        let address = address + fsize;

        let mem = Phys::allocate(asize, true)?;

        let cdata = image.read(fend, csize)?;
        unsafe { mem.write(0, &cdata) }?;

        log::trace!(
            "Alloc {:#x}~{:#x} -> {:#x}",
            fend,
            fend + asize,
            address - base
        );
        virt.map_phys(Some(address - base), mem, flags)?;
    }
    Ok(())
}

fn load_segs<'a>(
    mut image: Image<'a>,
    bootfs: Directory<'a>,
    bootfs_phys: &Phys,
    root: &Virt,
) -> Result<(NonNull<u8>, usize, Flags), Error> {
    let mut found_interp = false;
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
                found_interp = true;
            }
            None => break file,
        }
    };
    assert!(found_interp, "Executables cannot be directly executed");

    let is_dynamic = match file.kind() {
        ObjectKind::Dynamic => true,
        ObjectKind::Executable => false,
        _ => unimplemented!(),
    };

    let (min, max) = { file.raw_segments().iter() }.fold((usize::MAX, 0), |(min, max), seg| {
        (
            min.min(seg.p_vaddr(file.endian()) as usize),
            max.max((seg.p_vaddr(file.endian()) + seg.p_memsz(file.endian())) as usize),
        )
    });
    let layout = unsafe { Layout::from_size_align_unchecked(max - min, PAGE_SIZE).pad_to_align() };
    let virt = root.allocate(is_dynamic.then(|| min), layout)?;

    let base = virt.base().as_ptr() as usize;
    let entry = file.entry() as usize + if is_dynamic { base } else { 0 };

    let (mut stack_size, mut stack_flags) = (
        DEFAULT_STACK_SIZE,
        Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS,
    );

    for seg in file.raw_segments() {
        match seg.p_type(file.endian()) {
            PT_LOAD if seg.p_memsz(file.endian()) > 0 => {
                load_seg(
                    &image.phys,
                    &virt,
                    file.endian(),
                    seg,
                    if is_dynamic { 0 } else { base },
                )?;
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
    bootfs_phys: &Phys,
    root: &Virt,
) -> Result<(NonNull<u8>, NonNull<u8>), Error> {
    let (entry, stack_size, stack_flags) = load_segs(image, bootfs, bootfs_phys, root)?;

    let stack = {
        let size = stack_size + STACK_PROTECTOR_SIZE * 2;

        let virt = root.allocate(None, unsafe {
            Layout::from_size_align_unchecked(size, PAGE_SIZE)
        })?;
        let stack_phys = Phys::allocate(stack_size, true)?;
        let stack_ptr = virt.map(
            Some(PAGE_SIZE),
            stack_phys,
            0,
            unsafe { Layout::from_size_align_unchecked(stack_size, PAGE_SIZE) },
            stack_flags,
        )?;

        let base = stack_ptr.as_non_null_ptr();
        unsafe { NonNull::new_unchecked(base.as_ptr().add(stack_size)) }
    };

    Ok((entry, stack))
}
