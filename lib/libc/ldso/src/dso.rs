use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU32, Ordering::*},
};

use cstr_core::CStr;
use iter_ex::PointerIterator;
use solvent::prelude::*;

use crate::{elf::*, load_address, vdso_map};

type E = Endianness;

static mut SELF: MaybeUninit<Dso> = MaybeUninit::uninit();
static mut VDSO: MaybeUninit<Dso> = MaybeUninit::uninit();

#[derive(Debug, Default)]
pub struct Dso {
    id: u32,
    base: usize,
    name: &'static str,
    is_dyn: bool,
    endian: E,
    dyn_offset: usize,

    phdr: PointerIterator<ProgramHeader64<E>>,

    loads_start: usize,
    loads_end: usize,

    relro_start: usize,
    relro_end: usize,

    syms_offset: usize,
    strs_offset: usize,
    got_offset: usize,
    gnu_hash_offset: usize,

    so_name: &'static CStr,
}

impl Dso {
    /// # Safety
    ///
    /// The caller must ensure that the mapping for the DSO is initialized.
    unsafe fn new_mapped(base: usize, name: &'static str) -> Dso {
        let mut dso = Dso {
            base,
            name,
            ..Default::default()
        };
        // SAFETY: The safety check is guaranteed by the caller.
        unsafe { init_mapped(&mut dso) };
        dso
    }

    fn ptr<T>(&self, offset: usize) -> *mut T {
        let addr = if self.is_dyn {
            self.base + offset
        } else {
            offset
        };
        addr as *mut T
    }
}

fn next_id() -> u32 {
    static ID: AtomicU32 = AtomicU32::new(1);
    ID.fetch_add(1, SeqCst)
}

/// # Safety
///
/// The caller must ensure that the mapping for the DSO is initialized.
unsafe fn init_mapped(dso: &mut Dso) {
    // SAFETY: The mapping is already initialized.
    let header = unsafe { ptr::read(dso.base as *const FileHeader64<E>) };
    assert!(header.e_ident.magic == ELFMAG);
    let e = if header.e_ident.data == ELFDATA2MSB {
        E::Big
    } else {
        E::Little
    };
    dso.endian = e;
    dso.is_dyn = match header.e_type.get(e) {
        ET_DYN => true,
        ET_EXEC => false,
        _ => unimplemented!(),
    };

    let mut loads_start = usize::MAX;
    let mut loads_end = 0;

    dso.phdr = PointerIterator::new(
        dso.ptr::<ProgramHeader64<E>>(header.e_phoff.get(e) as usize),
        header.e_phnum.get(e) as usize,
        header.e_phentsize.get(e) as usize,
    );
    for phdr in dso.phdr {
        // SAFETY: The mapping is already initialized.
        let phdr = unsafe { ptr::read(phdr) };
        let offset = phdr.p_vaddr.get(e) as usize;
        let len = phdr.p_memsz.get(e) as usize;
        match phdr.p_type.get(e) {
            PT_LOAD => {
                loads_start = loads_start.min(offset);
                loads_end = loads_end.max(offset + len);
            }
            PT_DYNAMIC => dso.dyn_offset = offset,
            PT_GNU_RELRO => {
                dso.relro_start = offset;
                dso.relro_end = offset + len;
            }
            _ => {}
        }
    }

    dso.loads_start = loads_start & !PAGE_MASK;
    dso.loads_end = (loads_end + PAGE_MASK) & !PAGE_MASK;

    dso.id = next_id();
    decode_dyn(dso);
}

unsafe fn decode_dyn(dso: &mut Dso) {
    let e = dso.endian;

    let mut dyn_ptr = dso.ptr::<Dyn64<E>>(dso.dyn_offset);
    loop {
        let dynamic = ptr::read(dyn_ptr);
        let tag = dynamic.d_tag.get(e) as u32;
        let val = dynamic.d_val.get(e) as usize;
        match tag {
            DT_NULL => break,
            DT_SYMTAB => dso.syms_offset = val,
            DT_STRTAB => dso.strs_offset = val,
            DT_PLTGOT => dso.got_offset = val,
            DT_GNU_HASH => dso.gnu_hash_offset = val,
            DT_SONAME => dso.so_name = CStr::from_ptr(dso.ptr(val)),
            _ => {}
        }
        dyn_ptr = dyn_ptr.add(1);
    }
}

pub fn init() {
    unsafe {
        SELF.write(Dso::new_mapped(load_address(), "libc.so"));
        VDSO.write(Dso::new_mapped(vdso_map(), "<VDSO>"));
    }
}
