use alloc::{boxed::Box, collections::BTreeSet, ffi::CString, vec::Vec};
use core::{
    alloc::Layout,
    cell::UnsafeCell,
    ffi::CStr,
    fmt,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    ptr::{self, NonNull},
    slice,
    sync::atomic::{self, AtomicU32, Ordering::*},
    time::Duration,
};

use canary::Canary;
use elfload::LoadedElf;
use rpc::load::{GetObject, GetObjectResponse as Response};
use solvent::prelude::{Channel, Object, Phys};
use spin::{Lazy, Mutex, Once};
use svrt::{HandleInfo, HandleType};

use crate::{cstr, elf::*, load_address, vdso_map};

static mut LDSO: MaybeUninit<Dso> = MaybeUninit::uninit();
static mut VDSO: MaybeUninit<Dso> = MaybeUninit::uninit();
static LDRPC: Lazy<Option<Channel>> = Lazy::new(|| {
    let handle =
        svrt::try_take_startup_handle(HandleInfo::new().with_handle_type(HandleType::LoadRpc))
            .ok()?;
    Some(unsafe { Channel::from_raw(handle) })
});

static mut DSO_LIST: MaybeUninit<Mutex<DsoList>> = MaybeUninit::uninit();

pub fn dso_list() -> &'static Mutex<DsoList> {
    unsafe { DSO_LIST.assume_init_ref() }
}

type IniFn = unsafe extern "C" fn();

#[derive(Debug)]
pub enum Error {
    SymbolLoad,
    ElfLoad(elfload::Error),
    DepGet(solvent::error::Error),
    Memory(usize, usize),
}

#[derive(Copy, Clone)]
pub enum DsoBase {
    Dyn(usize),
    Exec(usize),
}

impl DsoBase {
    pub fn new(base: usize, e_type: u16) -> Self {
        match e_type {
            ET_DYN => DsoBase::Dyn(base),
            ET_EXEC => DsoBase::Exec(base),
            _ => unimplemented!(),
        }
    }

    pub fn get(self) -> usize {
        match self {
            DsoBase::Dyn(base) => base,
            DsoBase::Exec(base) => base,
        }
    }

    pub fn ptr<T>(&self, offset: usize) -> *mut T {
        let addr = match self {
            DsoBase::Dyn(base) => base + offset,
            DsoBase::Exec(_) => offset,
        };
        addr as *mut T
    }
}

impl fmt::Debug for DsoBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DsoBase::Dyn(b) => write!(f, "Dyn({:#x})", b),
            DsoBase::Exec(b) => write!(f, "Exec({:#x})", b),
        }
    }
}

#[derive(Debug, Default)]
struct DsoLink {
    next: Option<NonNull<Dso>>,
    prev: Option<NonNull<Dso>>,
}

#[derive(Debug)]
pub struct Dso {
    pub(crate) canary: Canary<Dso>,

    link: UnsafeCell<DsoLink>,
    fini_link: UnsafeCell<DsoLink>,

    _id: u32,
    base: DsoBase,
    name: &'static CStr,

    dynamic: &'static [Dyn],
    syms: Symbols<'static>,
    tls: Option<usize>,

    relocate: Once,
    init: Once,
    fini: Once,
}

impl Dso {
    /// # Safety
    ///
    /// The caller must ensure that the mapping for the DSO is initialized.
    unsafe fn new_static(base: usize, name: &'static CStr) -> Result<Dso, Error> {
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
            .unwrap_or(&[]);

        let syms = Symbols::from_dynamic(&base, dynamic, None).ok_or(Error::SymbolLoad)?;

        Ok(Dso {
            canary: Canary::new(),
            link: Default::default(),
            fini_link: Default::default(),
            _id: Self::next_id(),
            base,
            name,
            dynamic,
            syms,
            tls: None,
            relocate: Once::new(),
            init: Once::new(),
            fini: Once::new(),
        })
    }

    pub fn load(
        phys: &Phys,
        name: impl Into<CString>,
        prog: bool,
    ) -> Result<(LoadedElf, NonNull<Dso>), Error> {
        let mut dso_list = dso_list().lock();
        let names = dso_list.names.clone();
        Self::load_dso(phys, name.into(), prog, &mut dso_list)
            .inspect_err(|_| dso_list.names = names)
    }

    fn load_dso(
        phys: &Phys,
        name: CString,
        prog: bool,
        dso_list: &mut DsoList,
    ) -> Result<(LoadedElf, NonNull<Dso>), Error> {
        let elf = elfload::load(phys, true, &svrt::root_virt()).map_err(Error::ElfLoad)?;

        let base = DsoBase::new(elf.range.start, if elf.is_dyn { ET_DYN } else { ET_EXEC });
        log::debug!("{:?}", base);

        let dynamic = elf
            .dynamic
            .and_then(|seg| unsafe {
                let size = mem::size_of::<Dyn>();
                slice::from_raw_parts(
                    base.ptr::<Dyn>(seg.p_vaddr as usize),
                    seg.p_memsz as usize / size,
                )
                .split(|d| d.d_tag == DT_NULL)
                .next()
            })
            .unwrap_or(&[]);

        let syms =
            Symbols::from_dynamic(&base, dynamic, Some(elf.sym_len)).ok_or(Error::SymbolLoad)?;

        dso_list.names.insert(name.clone());
        Self::load_deps(dynamic, &syms, dso_list)?;

        let name = unsafe { CStr::from_ptr(CString::into_raw(name)) };

        let mut dso = Dso {
            canary: Canary::new(),
            link: Default::default(),
            fini_link: Default::default(),
            _id: Self::next_id(),
            base,
            name,
            dynamic,
            syms,
            tls: None,
            relocate: Once::new(),
            init: Once::new(),
            fini: Once::new(),
        };
        if let Some(ref tls) = elf.tls {
            dso_list.load_tls(&mut dso, tls, prog)?
        }
        let ptr = dso_list.push(dso, prog);
        Ok((elf, ptr))
    }

    fn load_deps(dynamic: &[Dyn], syms: &Symbols, dso_list: &mut DsoList) -> Result<(), Error> {
        let deps = { dynamic.iter() }
            .filter_map(|d| {
                (d.d_tag == DT_NEEDED)
                    .then(|| unsafe { syms.get_str(d.d_val as usize) })
                    .filter(|name| !dso_list.names.contains(*name))
                    .map(CString::from)
            })
            .collect::<Vec<_>>();
        log::debug!("Dependencies: {:?}", deps);
        let objs = get_object(deps.clone())?;
        for (phys, name) in objs.into_iter().zip(deps.into_iter()) {
            Self::load_dso(&phys, name, false, dso_list)?;
        }
        Ok(())
    }

    fn next_id() -> u32 {
        static ID: AtomicU32 = AtomicU32::new(1);
        ID.fetch_add(1, SeqCst)
    }
}

impl Dso {
    fn dyn_val(&self, tag: u64) -> Option<usize> {
        self.dynamic
            .iter()
            .find_map(|d| (d.d_tag == tag).then_some(d.d_val as usize))
    }

    fn dyn_ptr<T>(&self, tag: u64) -> Option<*mut T> {
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

    prog: Option<NonNull<Dso>>,
    fini: Option<NonNull<Dso>>,

    names: BTreeSet<CString>,
    tls: Vec<Tls>,

    preinit: Once,
}

unsafe impl Send for DsoList {}

impl DsoList {
    unsafe fn new(head: &Dso, tail: &Dso) -> Self {
        (*head.link.get()).next = Some(NonNull::from(tail));
        (*tail.link.get()).prev = Some(NonNull::from(head));
        let list = DsoList {
            head: Some(NonNull::from(head)),
            tail: Some(NonNull::from(tail)),
            prog: None,
            fini: None,
            names: BTreeSet::new(),
            tls: Vec::new(),
            preinit: Once::new(),
        };
        list.relocate_dso(head);
        list.relocate_dso(tail);
        list
    }

    fn iter(&self) -> DsoIter {
        DsoIter {
            cur: self.head,
            _marker: PhantomData,
        }
    }

    fn program(&self) -> Option<&Dso> {
        unsafe { self.prog.map(|p| p.as_ref()) }
    }

    pub fn tls(&mut self, id: usize) -> Option<&mut Tls> {
        self.tls.get_mut(id)
    }

    pub fn find_symbol(
        &self,
        name: &CStr,
        except: Option<&Dso>,
        needs_def: bool,
    ) -> Option<(&Dso, Sym)> {
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

            let sym = match dso.syms.get_by_name_hashed(name, ghash) {
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

    fn get_symbol_value_hashed(&self, dso: &Dso, name: &CStr, ghash: u32) -> Option<*mut u8> {
        match dso.syms.get_by_name_hashed(name, ghash) {
            Some(sym) if st_type(sym.st_info) == STT_TLS => {
                let base = self.tls.get(dso.tls?)?.as_ptr();
                Some(unsafe { base.add(sym.st_value as usize) })
            }
            Some(sym) if check_type(st_type(sym.st_info)) => {
                Some(dso.base.ptr::<u8>(sym.st_value as usize))
            }
            _ => None, // TODO: Find the symbol is its dependencies.
        }
    }

    pub fn get_symbol_value(&self, dso: Option<&Dso>, name: &CStr) -> Option<*mut u8> {
        if let Some(dso) = dso {
            let ghash = GnuHash::hash(name.to_bytes());

            return self.get_symbol_value_hashed(dso, name, ghash);
        }

        let (dso, sym) = self.find_symbol(name, None, false)?;
        if st_type(sym.st_info) == STT_TLS {
            let base = self.tls.get(dso.tls?)?.as_ptr();
            Some(unsafe { base.add(sym.st_value as usize) })
        } else {
            Some(dso.base.ptr::<u8>(sym.st_value as usize))
        }
    }

    fn relocate_one(&self, dso: &Dso, reloc: Reloc) -> bool {
        let reloc_ptr = dso.base.ptr(reloc.offset);

        let sym = match dso.syms.get(reloc.sym_index) {
            Some(sym) => *sym,
            None => return false,
        };
        let name = &unsafe { dso.syms.get_str(sym.st_name as usize) };
        let def = if reloc.sym_index == 0 {
            None
        } else if st_type(sym.st_info) == STT_SECTION {
            Some((dso, sym))
        } else {
            let def = self.find_symbol(
                name,
                (reloc.ty == R_X86_64_COPY).then_some(dso),
                reloc.ty == R_X86_64_JUMP_SLOT,
            );
            if def.is_none()
                && (sym.st_shndx as u32 != SHN_UNDEF || st_bind(sym.st_info) != STB_WEAK)
            {
                log::error!(
                    "Symbol {:?} not found when relocating in DSO {:?}",
                    name,
                    dso.name
                );
                panic!("Failed to relocate symbols");
            }
            def
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
                R_X86_64_DTPMOD64 => {
                    let dso = def.map(|(dso, _)| dso).unwrap_or(dso);
                    *reloc_ptr = dso.tls.expect("No TLS available");
                }
                R_X86_64_DTPOFF64 => {
                    let (dso, sym) = def.expect("No definition found for DTPOFF64");
                    let tls_subtrahend = self.tls[dso.tls.expect("No TLS available")]
                        .chunk_layout()
                        .size();
                    *reloc_ptr = (sym.st_value as usize + reloc.addend).wrapping_sub(tls_subtrahend)
                }
                R_X86_64_TPOFF64 => {
                    let (dso, sym) = def.expect("No definition found for TPOFF64");
                    let start = self.tls[dso.tls.expect("No TLS available")].as_ptr();
                    let tls_addend =
                        (start as usize).wrapping_sub(Tcb::current().static_base as usize);
                    *reloc_ptr = (sym.st_value as usize + reloc.addend).wrapping_add(tls_addend)
                }
                _ => unimplemented!("relocate other types: {:?}", reloc.ty),
            }
        }
        false
    }

    fn relocate_dso(&self, dso: &Dso) {
        fn r<T>(list: &DsoList, dso: &Dso, tag_offset: u64, tag_size: u64)
        where
            for<'a> &'a T: Into<Reloc>,
        {
            if let Some(iter) = unsafe { dso.dyn_slice::<T>(tag_offset, tag_size) } {
                for reloc in iter {
                    if list.relocate_one(dso, reloc.into()) {
                        break;
                    }
                }
            }
        }

        dso.relocate.call_once(|| {
            if dso.base.get() != load_address() {
                if let Some((offset, size)) = dso.dyn_val(DT_RELR).zip(dso.dyn_val(DT_RELRSZ)) {
                    unsafe { apply_relr(dso.base.ptr(0), dso.base.ptr(offset), size) }
                }
            }

            unsafe {
                r::<Rel>(self, dso, DT_REL, DT_RELSZ);
                r::<Rela>(self, dso, DT_RELA, DT_RELASZ);

                match dso.dyn_val(DT_PLTREL).unwrap_or_default() as u64 {
                    DT_RELA => r::<Rela>(self, dso, DT_JMPREL, DT_PLTRELSZ),
                    DT_REL => r::<Rel>(self, dso, DT_JMPREL, DT_PLTRELSZ),
                    v if v > 0 => log::warn!("Unknown DT_PLTREL value: {}", v),
                    _ => {}
                };
            }
        });
    }

    pub fn push(&mut self, dso: Dso, prog: bool) -> NonNull<Dso> {
        let dso = Box::leak(Box::new(dso));

        unsafe {
            dso.link.get_mut().next = None;
            dso.link.get_mut().prev = self.tail;
            let node = Some(NonNull::from(&*dso));

            match self.tail {
                None => self.head = node,
                // Not creating new mutable (unique!) references overlapping `element`.
                Some(tail) => (*(*tail.as_ptr()).link.get()).next = node,
            }

            self.tail = node;
        }

        self.relocate_dso(dso);

        if prog {
            self.prog = Some(dso.into());
        }
        dso.into()
    }

    pub fn pop(&mut self, dso: NonNull<Dso>) -> Option<Dso> {
        if unsafe { dso == LDSO.assume_init_ref().into() || dso == VDSO.assume_init_ref().into() } {
            return None;
        }

        let mut cur = self.head;
        loop {
            cur = match cur {
                Some(mut cur) if cur == dso => {
                    // SAFETY: The pointer is ours.
                    unsafe {
                        // These two are ours now, and we can create &mut s.
                        let link = &mut *(cur.as_mut().link.get());

                        // Not creating new mutable (unique!) references overlapping `element`.
                        match link.prev {
                            Some(mut prev) => unsafe {
                                (*(prev.as_mut().link.get())).next = link.next
                            },
                            // These nodes start with the head.
                            None => self.head = link.next,
                        }

                        match link.next {
                            Some(mut next) => unsafe {
                                (*(next.as_mut().link.get())).prev = link.prev
                            },
                            // These nodes end with the tail.
                            None => self.tail = link.prev,
                        }

                        link.prev = None;
                        link.next = None;
                    };

                    // SAFETY: The pointer will be no longer read again and the ownership is moved
                    // to `value`.
                    let value = unsafe { cur.as_ptr().read() };
                    // SAFETY: The pointer is ours.
                    let _ = unsafe { Box::from_raw(cur.as_ptr()) };

                    break Some(value);
                }
                // SAFETY: The pointer is allocated from the arena.
                Some(cur) => unsafe { (*cur.as_ref().link.get()).next },
                None => break None,
            }
        }
    }

    fn push_fini(fini: &mut Option<NonNull<Dso>>, dso: &Dso) {
        unsafe {
            (*dso.link.get()).prev = None;
            (*dso.link.get()).next = *fini;

            *fini = Some(dso.into());
        }
    }

    pub fn do_init(&mut self) {
        if let Some(preinit_array) = self.program().and_then(|prog| unsafe {
            prog.dyn_slice::<IniFn>(DT_PREINIT_ARRAY, DT_PREINIT_ARRAYSZ)
        }) {
            self.preinit
                .call_once(|| unsafe { preinit_array.iter().for_each(|p| p()) });
        }

        let mut cur = self.head;
        while let Some(ptr) = cur {
            let dso = unsafe { ptr.as_ref() };

            dso.init.call_once(|| unsafe {
                if let Some(init) = dso.dyn_ptr::<IniFn>(DT_INIT) {
                    (*init)();
                }

                if let Some(init_arr) = dso.dyn_slice::<IniFn>(DT_INIT_ARRAY, DT_INIT_ARRAYSZ) {
                    init_arr.iter().for_each(|i| i());
                }
                Self::push_fini(&mut self.fini, dso);
            });

            cur = unsafe { (*dso.link.get()).next };
        }
    }

    pub fn do_fini(&self) {
        let mut cur = self.fini;
        while let Some(ptr) = cur {
            let dso = unsafe { ptr.as_ref() };

            if dso.init.is_completed() {
                dso.fini.call_once(|| unsafe {
                    if let Some(fini_arr) = dso.dyn_slice::<IniFn>(DT_FINI_ARRAY, DT_FINI_ARRAYSZ) {
                        fini_arr.iter().rev().for_each(|f| f());
                    }

                    if let Some(fini) = dso.dyn_ptr::<IniFn>(DT_FINI) {
                        (*fini)();
                    }
                });
            }

            cur = unsafe { (*dso.fini_link.get()).next };
        }
    }

    fn load_tls(&mut self, dso: &mut Dso, tls: &ProgramHeader, prog: bool) -> Result<(), Error> {
        let layout = Layout::from_size_align(tls.p_memsz as usize, tls.p_align as usize)
            .map_err(|_| Error::Memory(tls.p_memsz as usize, tls.p_align as usize))?;

        let tdata = dso.base.ptr::<u8>(tls.p_offset as usize);
        let init_data = NonNull::slice_from_raw_parts(
            unsafe { NonNull::new_unchecked(tdata) },
            tls.p_filesz as usize,
        );

        let tls = Tls::new(init_data, layout)
            .map_err(|_| Error::Memory(layout.size(), layout.align()))?;
        if prog {
            unsafe {
                Tcb::current().static_base = tls.static_base();
            }
        }
        dso.tls = Some(self.tls.len());
        self.tls.push(tls);

        Ok(())
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

pub fn init() -> Result<(), Error> {
    unsafe {
        let ldso = LDSO.write(Dso::new_static(load_address(), cstr!("ld-oceanic.so"))?);
        let vdso = VDSO.write(Dso::new_static(vdso_map(), cstr!("<VDSO>"))?);
        DSO_LIST.write(Mutex::new(DsoList::new(ldso, vdso)))
    };
    atomic::fence(SeqCst);

    unsafe {
        let dso_list = DSO_LIST.assume_init_mut().get_mut();
        dso_list
            .names
            .extend([cstr!("libldso.so").into(), cstr!("libh2o.so").into()]);

        Tcb::init_current(0);
    }
    Ok(())
}

pub fn get_object(path: Vec<CString>) -> Result<Vec<Phys>, Error> {
    let ldrpc = LDRPC
        .as_ref()
        .ok_or(Error::DepGet(solvent::error::ENOENT))?;
    let resp = rpc::call::<GetObject>(ldrpc, path.into(), Duration::MAX).map_err(Error::DepGet)?;
    match resp {
        Response::Success(objs) => Ok(objs),
        Response::Error { not_found_index } => {
            log::error!("DT_NEEDED Library at index {} not found", not_found_index);
            Err(Error::DepGet(solvent::error::ENOENT))
        }
    }
}

fn check_type(st_type: u8) -> bool {
    st_type == STT_NOTYPE
        || st_type == STT_COMMON
        || st_type == STT_FUNC
        || st_type == STT_OBJECT
        || st_type == STT_TLS
}
