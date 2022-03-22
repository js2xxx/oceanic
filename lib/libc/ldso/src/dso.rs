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

static mut LDSO: MaybeUninit<Dso> = MaybeUninit::uninit();
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
    dynamic: &'static [Dyn],
    gnu_hash: GnuHash<'static>,

    relocate: Once<Result>,
}

impl Dso {
    /// # Safety
    ///
    /// The caller must ensure that the mapping for the DSO is initialized.
    unsafe fn new_static(base: usize, name: &'static str) -> Result<Dso> {
        // SAFETY: The mapping is already initialized.
        let header = unsafe { ptr::read(base as *const Header) };
        assert_eq!(&header.e_ident[..SELFMAG], ELFMAG);
        assert_eq!(header.e_ident[EI_DATA], ELFDATA2LSB);

        let base = DsoBase::new(base, header.e_type);

        // Sections are not loaded by default, so it can't be used at runtime.
        let segments = unsafe {
            assert_eq!(header.e_phentsize as usize, mem::size_of::<ProgramHeader>());

            slice::from_raw_parts(
                base.ptr::<ProgramHeader>(header.e_phoff as usize),
                header.e_phnum as usize,
            )
        };

        let dynamic = segments
            .iter()
            .find(|seg| seg.p_type == PT_DYNAMIC)
            .and_then(|seg| unsafe {
                let size = mem::size_of::<Dyn>();
                slice::from_raw_parts(
                    base.ptr::<Dyn>(seg.p_vaddr as usize),
                    seg.p_memsz as usize / size,
                )
                .split(|d| d.d_tag == DT_NULL)
                .next()
            })
            .ok_or(Error::ENOEXEC)?;

        let gnu_hash = {
            let (gnu_hash, symt_len) = Self::gnu_hash_data(dynamic, &base).ok_or(Error::ENOENT)?;
            let sym = unsafe {
                let ptr = base.ptr(
                    dynamic
                        .iter()
                        .find(|d| d.d_tag == DT_SYMTAB)
                        .map(|d| d.d_val as usize)
                        .expect("Failed to find symbol table"),
                );
                slice::from_raw_parts(ptr, symt_len)
            };
            GnuHash::from_raw_table(gnu_hash, sym).map_err(|_| Error::ENOEXEC)?
        };

        Ok(Dso {
            link: Default::default(),
            id: Self::next_id(),
            base,
            name,
            segments,
            dynamic,
            gnu_hash,
            relocate: Once::new(),
        })
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

    fn gnu_hash_data<'a>(dynamic: &'a [Dyn], base: &DsoBase) -> Option<(&'a [u8], usize)> {
        let offset = dynamic
            .iter()
            .find_map(|d| (d.d_tag == DT_GNU_HASH).then(|| d.d_val as usize))?;
        let base = base.ptr::<u8>(offset);
        let mut ptr = base;

        unsafe {
            let [bucket_count, sym_base, bloom_count, _] = ptr::read(ptr.cast::<[u32; 4]>());
            ptr = ptr.add(4 * mem::size_of::<u32>());

            // bloom_filters: [u64; bloom_count]
            ptr = ptr.add(bloom_count as usize * mem::size_of::<u64>());

            let buckets = slice::from_raw_parts(ptr.cast::<u32>(), bucket_count as usize);
            let mut ptr = ptr
                .add(bucket_count as usize * mem::size_of::<u32>())
                .cast::<u32>();

            let mut max_sym = buckets.iter().max().copied().unwrap();
            ptr = ptr.add((max_sym - sym_base) as usize);

            loop {
                let value = *ptr;
                max_sym += 1;
                ptr = ptr.add(1);
                if value & 1 != 0 {
                    let len = ptr.cast::<u8>().offset_from(base) as usize;
                    let data = slice::from_raw_parts(base, len);
                    break Some((data, max_sym as usize));
                }
            }
        }
    }

    fn dyn_val(&self, tag: u64) -> Option<usize> {
        self.dynamic
            .iter()
            .find_map(|d| (d.d_tag == tag).then(|| d.d_val as usize))
    }

    unsafe fn dyn_ptr<T>(&self, tag: u64) -> Option<*mut T> {
        self.dyn_val(tag).map(|offset| self.base.ptr(offset))
    }

    unsafe fn dyn_slice<T>(&self, tag_offset: u64, tag_size: u64) -> Option<&[T]> {
        self.dyn_ptr(tag_offset).and_then(|ptr| {
            self.dyn_val(tag_size)
                .map(|size| unsafe { slice::from_raw_parts(ptr, size / mem::size_of::<T>()) })
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

    pub fn relocate(&self) -> Result {
        let mut cursor = self.head;
        while let Some(ptr) = cursor {
            let dso = unsafe { ptr.as_ref() };

            let ret = *dso.relocate.call_once(|| {
                if dso.base.get() != load_address() {
                    if let Some((offset, size)) = dso.dyn_val(DT_RELR).zip(dso.dyn_val(DT_RELRSZ)) {
                        unsafe { apply_relr(dso.base.ptr(0), dso.base.ptr(offset), size) }
                    }
                }

                let symbols = unsafe { dso.dyn_ptr::<Sym>(DT_SYMTAB) }.ok_or(Error::ENOENT)?;
                let strings = unsafe { dso.dyn_ptr::<i8>(DT_STRTAB) }.ok_or(Error::ENOENT)?;

                let rel = unsafe { dso.dyn_slice::<Rel>(DT_REL, DT_RELSZ) }.unwrap_or(&[]);
                let rela = unsafe { dso.dyn_slice::<Rela>(DT_RELA, DT_RELASZ) }.unwrap_or(&[]);

                for reloc in Reloc::from_iter(rel.iter(), rela.iter()) {
                    let reloc_ptr = dso.base.ptr(reloc.offset);

                    let sym = unsafe { ptr::read(symbols.add(reloc.sym_index)) };
                    let def = if reloc.sym_index != 0 {
                        let name = unsafe { CStr::from_ptr(strings.add(sym.st_name as usize)) };
                        if st_type(sym.st_info) == STT_SECTION {
                            Some((dso, sym))
                        } else {
                            self.find_symbol(
                                name,
                                (reloc.ty == R_X86_64_COPY).then(|| dso),
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

                Ok(())
            });
            ret?;

            cursor = unsafe { (*dso.link.get()).next };
        }
        Ok(())
    }
}

pub fn init() {
    let list = unsafe {
        let ldso =
            LDSO.write(Dso::new_static(load_address(), "libc.so").expect("Failed to init LDSO"));
        let vdso = VDSO.write(Dso::new_static(vdso_map(), "<VDSO>").expect("Failed to init VDSO"));
        DsoList::new(ldso, vdso)
    };
    list.relocate().expect("Failed to relocate objects");
}
