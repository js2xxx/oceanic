use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr::{self, NonNull},
    slice,
    sync::atomic::{AtomicU32, Ordering::*},
};

use cstr_core::CStr;
use solvent::prelude::*;
use spin::Once;

use crate::{elf::*, load_address, vdso_map};

static mut SELF: MaybeUninit<Dso> = MaybeUninit::uninit();
static mut VDSO: MaybeUninit<Dso> = MaybeUninit::uninit();

#[derive(Debug, Copy, Clone)]
pub enum DsoBase {
    Dynamic(usize),
    Static(usize),
}

impl DsoBase {
    pub fn new(base: usize, e_type: u16) -> Self {
        match e_type {
            ET_DYN => DsoBase::Dynamic(base),
            ET_EXEC => DsoBase::Static(base),
            _ => unimplemented!(),
        }
    }

    pub fn get(self) -> usize {
        match self {
            DsoBase::Dynamic(base) => base,
            Self::Static(base) => base,
        }
    }

    pub fn ptr<T>(&self, offset: usize) -> *mut T {
        let addr = match self {
            DsoBase::Dynamic(base) => base + offset,
            DsoBase::Static(_) => offset,
        };
        addr as *mut T
    }
}

#[derive(Debug, Default)]
struct DsoLink {
    next: Option<NonNull<Dso>>,
    prev: Option<NonNull<Dso>>,
}

pub struct Dso {
    link: UnsafeCell<DsoLink>,

    id: u32,
    base: DsoBase,
    name: &'static str,

    segments: &'static [ProgramHeader],
    sections: &'static [SectionHeader],
    dynamic: &'static [Dyn],

    relocate: Once,
}

impl Dso {
    /// # Safety
    ///
    /// The caller must ensure that the mapping for the DSO is initialized.
    unsafe fn new_static(base: usize, name: &'static str) -> Dso {
        // SAFETY: The mapping is already initialized.
        let header = unsafe { ptr::read(base as *const Header) };
        assert_eq!(&header.e_ident[..SELFMAG], ELFMAG);
        assert_eq!(header.e_ident[EI_DATA], ELFDATA2LSB);

        let base = DsoBase::new(base, header.e_type);

        let (segments, sections) = unsafe {
            assert_eq!(header.e_phentsize as usize, mem::size_of::<ProgramHeader>());
            assert_eq!(header.e_shentsize as usize, mem::size_of::<SectionHeader>());
            (
                slice::from_raw_parts(
                    base.ptr::<ProgramHeader>(header.e_phoff as usize),
                    header.e_phnum as usize,
                ),
                slice::from_raw_parts(
                    base.ptr::<SectionHeader>(header.e_shoff as usize),
                    header.e_shnum as usize,
                ),
            )
        };

        let dynamic = segments
            .iter()
            .find(|seg| seg.p_type == ET_DYN.into())
            .and_then(|seg| unsafe {
                let size = mem::size_of::<Dyn>();
                slice::from_raw_parts(
                    base.ptr::<Dyn>(seg.p_vaddr as usize),
                    seg.p_memsz as usize / size,
                )
                .split(|d| d.d_tag == DT_NULL)
                .next()
            });

        Dso {
            link: Default::default(),
            id: Self::next_id(),
            base,
            name,
            segments,
            sections,
            dynamic: dynamic.unwrap_or(&[]),
            relocate: Once::new(),
        }
    }

    fn next_id() -> u32 {
        static ID: AtomicU32 = AtomicU32::new(1);
        ID.fetch_add(1, SeqCst)
    }
}

impl Dso {
    pub fn segment(&self, index: usize) -> Option<&ProgramHeader> {
        self.segments.get(index)
    }

    pub fn section(&self, index: usize) -> Option<&SectionHeader> {
        self.sections.get(index)
    }

    pub fn section_by_addr(&self, addr: *mut u8) -> Option<&SectionHeader> {
        self.sections.iter().find(|section| {
            let base = self.base.ptr(section.sh_addr as usize);
            let end = self.base.ptr((section.sh_addr + section.sh_size) as usize);
            base <= addr && addr <= end
        })
    }

    unsafe fn section_data<T>(&self, section: &SectionHeader) -> &[T] {
        assert_eq!(section.sh_entsize as usize, mem::size_of::<T>());
        slice::from_raw_parts(
            self.base.ptr(section.sh_addr as usize),
            section.sh_size as usize / mem::size_of::<T>(),
        )
    }

    fn dyn_val(&self, tag: u64) -> Option<usize> {
        self.dynamic
            .iter()
            .find_map(|d| (d.d_tag == tag).then(|| d.d_val as usize))
    }

    unsafe fn section_by_dyn<T>(&self, tag: u64) -> Option<&[T]> {
        self.dyn_val(tag)
            .and_then(|offset| self.section_by_addr(self.base.ptr(offset)))
            .map(|section| self.section_data(section))
    }

    unsafe fn slice_by_dyn<T>(&self, tag_offset: u64, tag_size: u64) -> Option<&[T]> {
        self.dyn_val(tag_offset).and_then(|offset| {
            self.dyn_val(tag_size).map(|size| unsafe {
                slice::from_raw_parts(self.base.ptr(offset), size / mem::size_of::<T>())
            })
        })
    }
}

struct Reloc {
    offset: usize,
    addend: usize,
    ty: u32,
    sym_index: usize,
}

impl Reloc {
    fn from_iter<'a>(
        rel: impl Iterator<Item = &'a Rel> + 'a,
        rela: impl Iterator<Item = &'a Rela> + 'a,
    ) -> impl Iterator<Item = Reloc> + 'a {
        rel.map(Self::from).chain(rela.map(Self::from))
    }
}

impl From<&Rel> for Reloc {
    fn from(rel: &Rel) -> Self {
        Reloc {
            offset: rel.r_offset as usize,
            addend: 0,
            ty: r_type(rel.r_info),
            sym_index: r_sym(rel.r_info) as usize,
        }
    }
}

impl From<&Rela> for Reloc {
    fn from(rel: &Rela) -> Self {
        Reloc {
            offset: rel.r_offset as usize,
            addend: rel.r_addend as usize,
            ty: r_type(rel.r_info),
            sym_index: r_sym(rel.r_info) as usize,
        }
    }
}

pub struct DsoList {
    head: Option<NonNull<Dso>>,
    tail: Option<NonNull<Dso>>,
}

unsafe impl Send for DsoList {}

impl DsoList {
    unsafe fn new(head: &Dso, tail: &Dso) -> Self {
        (*head.link.get()).next = Some(NonNull::from(tail));
        (*tail.link.get()).prev = Some(NonNull::from(head));
        DsoList {
            head: Some(NonNull::from(head)),
            tail: Some(NonNull::from(tail)),
        }
    }

    pub fn find_symbol(
        &self,
        name: &CStr,
        except: Option<&Dso>,
        needs_def: bool,
    ) -> Option<(&Dso, Sym)> {
        let mut cursor = self.head;
        while let Some(ptr) = cursor {
            let dso = unsafe { ptr.as_ref() };

            if !matches!(except, Some(except) if ptr::eq(except, dso)) {
                unimplemented!("find_symbol")
            }

            cursor = unsafe { (*dso.link.get()).next };
        }
        None
    }

    pub fn relocate(&self) -> Result<()> {
        let mut cursor = self.head;
        while let Some(ptr) = cursor {
            let dso = unsafe { ptr.as_ref() };

            dso.relocate.call_once(|| {
                if dso.base.get() != load_address() {
                    if let Some((offset, size)) = dso.dyn_val(DT_RELR).zip(dso.dyn_val(DT_RELRSZ)) {
                        unsafe { apply_relr(dso.base.ptr(0), dso.base.ptr(offset), size) }
                    }
                }

                let symbols = unsafe { dso.section_by_dyn::<Sym>(DT_SYMTAB) }.unwrap_or(&[]);
                let strings = unsafe { dso.section_by_dyn::<i8>(DT_STRTAB) }.unwrap_or(&[]);

                let rel = unsafe { dso.slice_by_dyn::<Rel>(DT_REL, DT_RELSZ) }.unwrap_or(&[]);
                let rela = unsafe { dso.slice_by_dyn::<Rela>(DT_RELA, DT_RELASZ) }.unwrap_or(&[]);

                for reloc in Reloc::from_iter(rel.iter(), rela.iter()) {
                    let reloc_ptr = dso.base.ptr(reloc.offset);

                    let sym = symbols[reloc.sym_index];
                    let def = if reloc.sym_index != 0 {
                        let name = unsafe { CStr::from_ptr(&strings[sym.st_name as usize]) };
                        if st_type(sym.st_info) == STT_SECTION {
                            Some((&*dso, sym))
                        } else {
                            self.find_symbol(
                                name,
                                (reloc.ty == R_X86_64_COPY).then(|| &*dso),
                                reloc.ty == R_X86_64_JUMP_SLOT,
                            )
                        }
                    } else {
                        None
                    };
                    let sym_val = def.as_ref().map_or(ptr::null_mut(), |(dso, sym)| {
                        dso.base.ptr(sym.st_value as usize)
                    });

                    unsafe {
                        match reloc.ty {
                            R_X86_64_NONE => break,
                            R_X86_64_64 | R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                                *reloc_ptr = sym_val as usize + reloc.addend
                            }
                            R_X86_64_RELATIVE => *reloc_ptr = dso.base.get() + reloc.addend,
                            R_X86_64_COPY => {
                                (reloc_ptr as *mut u8)
                                    .copy_from_nonoverlapping(sym_val, sym.st_size as usize);
                            }
                            R_X86_64_PC32 => {
                                *reloc_ptr = sym_val as usize + reloc.addend - reloc_ptr as usize
                            }
                            _ => unimplemented!("relocate other types"),
                        }
                    }
                }
            });

            cursor = unsafe { (*dso.link.get()).next };
        }
        Ok(())
    }
}

pub fn init() {
    let list = unsafe {
        let ldso = SELF.write(Dso::new_static(load_address(), "libc.so"));
        let vdso = VDSO.write(Dso::new_static(vdso_map(), "<VDSO>"));
        DsoList::new(ldso, vdso)
    };
    list.relocate().expect("Failed to relocate objects");
}
