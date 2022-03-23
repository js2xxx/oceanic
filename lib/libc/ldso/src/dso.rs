use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    ptr::{self, NonNull},
    slice,
    sync::atomic::{self, AtomicU32, Ordering::*},
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

    relocate: Once,
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

        let gnu_hash = base.ptr(
            dynamic
                .iter()
                .find_map(|d| (d.d_tag == DT_GNU_HASH).then(|| d.d_val as usize))
                .ok_or(Error::ENOEXEC)?,
        );
        let syms = base.ptr(
            dynamic
                .iter()
                .find_map(|d| (d.d_tag == DT_SYMTAB).then(|| d.d_val as usize))
                .ok_or(Error::ENOEXEC)?,
        );
        let strs = base.ptr(
            dynamic
                .iter()
                .find_map(|d| (d.d_tag == DT_STRTAB).then(|| d.d_val as usize))
                .ok_or(Error::ENOEXEC)?,
        );

        let gnu_hash = GnuHash::parse(gnu_hash, syms, strs);

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

    fn iter(&self) -> DsoIter {
        DsoIter {
            cur: self.head,
            _marker: PhantomData,
        }
    }

    pub fn find_symbol(
        &self,
        name: &CStr,
        except: Option<&Dso>,
        needs_def: bool,
    ) -> Option<(&Dso, Sym)> {
        fn check_type(st_type: u8) -> bool {
            st_type == STT_NOTYPE
                || st_type == STT_COMMON
                || st_type == STT_FUNC
                || st_type == STT_OBJECT
                || st_type == STT_TLS
        }
        fn check_bind(st_bind: u8) -> bool {
            st_bind == STB_GLOBAL || st_bind == STB_WEAK || st_bind == STB_GNU_UNIQUE
        }

        fn check_sym(sym: &Sym, needs_def: bool) -> bool {
            let (ty, bind) = (st_type(sym.st_info), st_bind(sym.st_info));
            // Needs an actual definition
            !(sym.st_shndx == 0 && (needs_def || ty == STT_TLS))
                && !(sym.st_value == 0 && ty != STT_TLS)
                && check_type(ty)
                && check_bind(bind)
        }

        let ghash = GnuHash::hash(name.to_bytes());
        let mut ret = None;

        for dso in self.iter() {
            if matches!(except, Some(except) if ptr::eq(except, dso)) {
                continue;
            }

            let sym = match dso.gnu_hash.get_hashed(name, ghash) {
                Some(sym) => sym,
                None => continue,
            };

            let bind = st_bind(sym.st_info);
            if !check_sym(sym, needs_def) {
                continue;
            }

            if ret.is_some() && bind == STB_WEAK {
                continue;
            }

            ret = Some((dso, *sym));
            if bind == STB_GLOBAL {
                break;
            }
        }
        ret
    }

    fn relocate_impl(&self, dso: &Dso, symbols: &[Sym], strings: *const i8, reloc: Reloc) -> bool {
        let reloc_ptr = dso.base.ptr(reloc.offset);

        let sym = symbols[reloc.sym_index];
        let def = if reloc.sym_index != 0 {
            if st_type(sym.st_info) == STT_SECTION {
                Some((dso, sym))
            } else {
                self.find_symbol(
                    unsafe { CStr::from_ptr(strings.add(sym.st_name as usize)) },
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
                R_X86_64_NONE => return true,
                R_X86_64_64 | R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                    *reloc_ptr = sym_val as usize + reloc.addend
                }
                R_X86_64_RELATIVE => *reloc_ptr = dso.base.get() + reloc.addend,
                R_X86_64_COPY => {
                    (reloc_ptr as *mut u8).copy_from_nonoverlapping(sym_val, sym.st_size as usize);
                }
                R_X86_64_PC32 => *reloc_ptr = sym_val as usize + reloc.addend - reloc_ptr as usize,
                _ => unimplemented!("relocate other types"),
            }
        }
        false
    }

    pub fn relocate(&self) {
        for dso in self.iter() {
            dso.relocate.call_once(|| {
                if dso.base.get() != load_address() {
                    if let Some((offset, size)) = dso.dyn_val(DT_RELR).zip(dso.dyn_val(DT_RELRSZ)) {
                        unsafe { apply_relr(dso.base.ptr(0), dso.base.ptr(offset), size) }
                    }
                }

                let symbols = dso.gnu_hash.symbols();
                let strings = dso.gnu_hash.string_base();

                let rel = unsafe { dso.dyn_slice::<Rel>(DT_REL, DT_RELSZ) }.unwrap_or(&[]);
                let rela = unsafe { dso.dyn_slice::<Rela>(DT_RELA, DT_RELASZ) }.unwrap_or(&[]);

                for reloc in { rel.iter().map(Reloc::from) }.chain(rela.iter().map(Reloc::from)) {
                    if self.relocate_impl(dso, symbols, strings, reloc) {
                        break;
                    }
                }
            });
        }
    }
}

#[derive(Clone, Copy)]
struct DsoIter<'a> {
    cur: Option<NonNull<Dso>>,
    _marker: PhantomData<&'a [Dso]>,
}

impl<'a> Iterator for DsoIter<'a> {
    type Item = &'a Dso;

    fn next(&mut self) -> Option<Self::Item> {
        self.cur.map(|cur| unsafe {
            // Need an unbound lifetime to get 'a
            let ret = &*cur.as_ptr();
            self.cur = (*ret.link.get()).next;
            ret
        })
    }
}

pub fn init() -> Result<DsoList> {
    let list = unsafe {
        let ldso = LDSO.write(Dso::new_static(load_address(), "libc.so")?);
        let vdso = VDSO.write(Dso::new_static(vdso_map(), "<VDSO>")?);
        DsoList::new(ldso, vdso)
    };
    list.relocate();
    atomic::fence(SeqCst);
    Ok(list)
}
