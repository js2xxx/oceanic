use core::{
    iter,
    mem::{self, MaybeUninit},
    ops::Index,
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

#[derive(Debug)]
pub struct DynBlock([usize; DT_NUM as usize]);

impl DynBlock {
    fn has_item(&self, index: u32) -> bool {
        self.0[0] & (1 << index as usize) != 0 && index < DT_NUM
    }

    fn get(&self, index: u32) -> Option<usize> {
        self.has_item(index).then(|| self.0[index as usize])
    }

    unsafe fn decode_mapped(mut dyn_ptr: *const Dyn64<E>, e: E) -> Self {
        let mut inner = [0; DT_NUM as usize];

        loop {
            let dynamic = ptr::read(dyn_ptr);
            let tag = dynamic.d_tag.get(e) as u32;
            let val = dynamic.d_val.get(e) as usize;
            match tag {
                DT_NULL => break DynBlock(inner),
                tag if tag < DT_NUM => {
                    inner[0] |= 1 << tag as usize;
                    inner[tag as usize] = val;
                }
                _ => {}
            }
            dyn_ptr = dyn_ptr.add(1);
        }
    }
}

impl Index<u32> for DynBlock {
    type Output = usize;

    fn index(&self, index: u32) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl Default for DynBlock {
    fn default() -> Self {
        Self([0; DT_NUM as usize])
    }
}

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

    dyn_block: DynBlock,

    so_name: Option<&'static CStr>,
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
        unsafe { Self::init(&mut dso) };
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

    fn next_id() -> u32 {
        static ID: AtomicU32 = AtomicU32::new(1);
        ID.fetch_add(1, SeqCst)
    }

    /// # Safety
    ///
    /// The caller must ensure that the mapping for the DSO is initialized.
    unsafe fn init(dso: &mut Dso) {
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

        dso.id = Self::next_id();
        dso.dyn_block = DynBlock::decode_mapped(dso.ptr::<Dyn64<E>>(dso.dyn_offset), dso.endian);
        dso.so_name =
            { dso.dyn_block.get(DT_SONAME) }.map(|offset| CStr::from_ptr(dso.ptr(offset)));
    }
}

pub fn init() {
    unsafe {
        SELF.write(Dso::new_mapped(load_address(), "libc.so"));
        VDSO.write(Dso::new_mapped(vdso_map(), "<VDSO>"));
    }
}

#[allow(dead_code)]
unsafe fn relocate_mapped(dso: &Dso) {
    if dso.base != load_address() {
        if let (Some(offset), Some(len)) =
            (dso.dyn_block.get(DT_RELR), dso.dyn_block.get(DT_RELRSZ))
        {
            apply_relr(dso.base as _, dso.ptr(offset), len);
        }
    }

    let rel = { dso.dyn_block.get(DT_REL) }.and_then(|offset| {
        dso.dyn_block.get(DT_RELSZ).map(|size| {
            PointerIterator::new_size(dso.ptr::<u64>(offset), size, mem::size_of::<u64>() * 2)
        })
    });
    let rela = { dso.dyn_block.get(DT_RELA) }.and_then(|offset| {
        dso.dyn_block.get(DT_RELASZ).map(|size| {
            PointerIterator::new_size(dso.ptr::<u64>(offset), size, mem::size_of::<u64>() * 3)
        })
    });

    for (rel, has_addend) in rel
        .iter()
        .flatten()
        .zip(iter::repeat(false))
        .chain(rela.iter().flatten().zip(iter::repeat(true)))
    {
        let ptr = dso.ptr::<usize>(ptr::read(rel) as usize);
        let (rel_type, sym_index) = {
            let synth = ptr::read(rel.add(1));
            ((synth & u32::MAX as u64) as u32, (synth >> 32) as usize)
        };
        let addend = if has_addend { ptr::read(rel.add(2)) } else { 0 } as usize;

        let sym_val = if sym_index != 0 {
            todo!("Find the symbol and get its value")
        } else {
            0
        };

        match rel_type {
            R_X86_64_NONE => {}
            R_X86_64_64 | R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => *ptr = sym_val + addend,
            R_X86_64_RELATIVE => *ptr = dso.base + addend,
            R_X86_64_COPY => *ptr = *(sym_val as *const usize),
            R_X86_64_PC32 => *ptr = sym_val + addend - ptr as usize,
            _ => todo!("Relocate other types"),
        }
    }
}
