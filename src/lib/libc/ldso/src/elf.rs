use alloc::{alloc::Global, boxed::Box};
use core::{
    alloc::{AllocError, Allocator, Layout},
    ffi::CStr,
    mem, ptr,
    ptr::NonNull,
    slice,
};

pub use goblin::elf64::{
    dynamic::*, header::*, program_header::*, reloc::*, section_header::*, sym::*, Note,
};

use crate::dso::DsoBase;

pub const PT_NUM: u32 = 10;

pub const DT_RELR: u64 = 36;
pub const DT_RELRSZ: u64 = 35;
pub const DT_RELRENT: u64 = 37;

pub const DT_NUM: u64 = 38;

/// # Safety
///
/// `base` must contains a valid reference to a statically mapped ELF structure
/// and `relr` must be the RELR entry in its dynamic section.
#[inline(always)]
pub unsafe fn apply_relr(base: *mut u8, relr: *const usize, size: usize) {
    let len = size / mem::size_of::<usize>();

    let mut i = 0;
    while i < len {
        let addr = base.add(*relr.add(i)).cast::<usize>();
        i += 1;

        *addr += base as usize;

        let mut addr = addr.add(1);
        while i < len && *relr.add(i) & 1 != 0 {
            let mut run = addr;
            addr = addr.add(usize::BITS as usize - 1);

            let mut bitmask = *relr.add(i) >> 1;
            i += 1;
            while bitmask != 0 {
                let skip = bitmask.trailing_zeros() as usize;
                run = run.add(skip);
                *run += base as usize;
                run = run.add(1);
                bitmask >>= skip + 1;
            }
        }
    }
}

#[derive(Debug)]
pub enum Symbols<'a> {
    GnuHashed(GnuHash<'a>),
    Raw(&'a [Sym], *const i8),
}

impl<'a> Symbols<'a> {
    pub fn from_raw(symtab: &'a [Sym], strtab: *const i8) -> Self {
        Self::Raw(symtab, strtab)
    }

    /// # Safety
    ///
    /// `ghash_ptr` and `dsym_ptr` must point to a valid GNU hash table and a
    /// symbol table.
    pub unsafe fn try_from_gnu(
        ghash_ptr: *const u8,
        sym_ptr: *const Sym,
        str_ptr: *const i8,
    ) -> Option<Self> {
        GnuHash::parse(ghash_ptr, sym_ptr, str_ptr).map(Self::GnuHashed)
    }

    pub fn from_dynamic(base: &DsoBase, dynamic: &[Dyn], sym_len: Option<usize>) -> Option<Self> {
        let sym_ptr = base.ptr(
            dynamic
                .iter()
                .find_map(|d| (d.d_tag == DT_SYMTAB).then_some(d.d_val as usize))?,
        );
        let str_ptr = base.ptr(
            dynamic
                .iter()
                .find_map(|d| (d.d_tag == DT_STRTAB).then_some(d.d_val as usize))?,
        );
        let gnu_hash = dynamic
            .iter()
            .find_map(|d| (d.d_tag == DT_GNU_HASH).then_some(d.d_val as usize))
            .map(|offset| base.ptr(offset));

        gnu_hash
            .and_then(|gnu_hash| unsafe { Self::try_from_gnu(gnu_hash, sym_ptr, str_ptr) })
            .or_else(|| {
                sym_len.map(|sym_len| {
                    Symbols::from_raw(unsafe { slice::from_raw_parts(sym_ptr, sym_len) }, str_ptr)
                })
            })
    }

    pub fn get_by_name_hashed(&self, name: &CStr, ghash: u32) -> Option<&'a Sym> {
        match self {
            Symbols::GnuHashed(ref ghtab) => ghtab.get_hashed(name, ghash),
            Symbols::Raw(syms, strs) => {
                let mut ret = None;
                for sym in *syms {
                    let start = sym.st_name as usize;
                    let sn = unsafe { CStr::from_ptr(strs.add(start)) };
                    if sn == name {
                        ret = Some(sym);
                    }
                }
                ret
            }
        }
    }

    pub fn get_by_name(&self, name: &CStr) -> Option<&'a Sym> {
        self.get_by_name_hashed(name, GnuHash::hash(name.to_bytes()))
    }

    pub fn get(&self, index: usize) -> Option<&'a Sym> {
        match self {
            Symbols::GnuHashed(ref ghtab) => ghtab.syms.get(index),
            Symbols::Raw(syms, _) => syms.get(index),
        }
    }

    /// # Safety
    ///
    /// The caller must ensure `st_name` is a valid name index of the symbol in
    /// this symbol table.
    pub unsafe fn get_str(&self, st_name: usize) -> &'a CStr {
        match self {
            Symbols::GnuHashed(ref ghtab) => CStr::from_ptr(ghtab.string_base().add(st_name)),
            Symbols::Raw(_, strs) => CStr::from_ptr(strs.add(st_name)),
        }
    }
}

#[derive(Debug)]
pub struct GnuHash<'a> {
    sym_base: u32,
    bloom_shift: u32,
    bloom_filters: &'a [u64],
    buckets: &'a [u32],
    chains: &'a [u32],
    syms: &'a [Sym],
    strs: *const i8,
}

impl<'a> GnuHash<'a> {
    /// # Safety
    ///
    /// `ghash_ptr` and `dsym_ptr` must point to a valid GNU hash table and a
    /// symbol table.
    pub unsafe fn parse(
        ghash_ptr: *const u8,
        sym_ptr: *const Sym,
        str_ptr: *const i8,
    ) -> Option<Self> {
        let [bucket_count, sym_base, bloom_count, bloom_shift] =
            ptr::read(ghash_ptr.cast::<[u32; 4]>());

        let bloom_base = ghash_ptr.add(4 * mem::size_of::<u32>()).cast::<u64>();

        let bucket_base = bloom_base.add(bloom_count as usize).cast::<u32>();
        let buckets = slice::from_raw_parts(bucket_base, bucket_count as usize);

        let chain_base = bucket_base.add(bucket_count as usize);
        let mut ptr = chain_base;

        let mut max_sym = buckets.iter().max().copied().unwrap();
        if max_sym == 0 {
            return None;
        }
        ptr = ptr.add((max_sym - sym_base) as usize);

        let (chain_len, sym_len) = loop {
            let value = *ptr;
            max_sym += 1;
            ptr = ptr.add(1);
            if value & 1 != 0 {
                break (ptr.offset_from(chain_base) as usize, max_sym as usize);
            }
        };

        Some(GnuHash {
            sym_base,
            bloom_shift,
            bloom_filters: slice::from_raw_parts(bloom_base, bloom_count as usize),
            buckets,
            chains: slice::from_raw_parts(chain_base, chain_len),
            syms: slice::from_raw_parts(sym_ptr, sym_len),
            strs: str_ptr,
        })
    }

    pub fn symbols(&self) -> &'a [Sym] {
        self.syms
    }

    pub fn string_base(&self) -> *const i8 {
        self.strs
    }

    pub fn hash(symbol: &[u8]) -> u32 {
        const HASH_SEED: u32 = 5381;
        symbol.iter().fold(HASH_SEED, |hash, &b| {
            hash.wrapping_mul(33).wrapping_add(u32::from(b))
        })
    }

    fn lookup(&self, name: &CStr, hash: u32) -> Option<&'a Sym> {
        const MASK_LB: u32 = 0xffff_fffe;
        let bucket = self.buckets[hash as usize % self.buckets.len()];

        // Empty hash chain, symbol not present
        if bucket < self.sym_base {
            return None;
        }

        // Walk the chain until the symbol is found or the chain is exhausted.
        let chain_idx = bucket - self.sym_base;
        let hash = hash & MASK_LB;

        let chains = &self.chains.get((chain_idx as usize)..)?;
        let syms = &self.syms.get((bucket as usize)..)?;
        for (h, symb) in chains.iter().zip(syms.iter()) {
            let sym_name = unsafe { CStr::from_ptr(self.strs.add(symb.st_name as usize)) };
            if hash == (h & MASK_LB) && name == sym_name {
                return Some(symb);
            }
            // Chain ends with an element with the lowest bit set to 1.
            if h & 1 != 0 {
                break;
            }
        }
        None
    }

    fn may_contain(&self, hash: u32) -> bool {
        const MASK: u32 = u64::BITS - 1;
        let hash2 = hash >> self.bloom_shift;
        // `x & (N - 1)` is equivalent to `x % N` iff `N = 2^y`.
        let bitmask = 1 << (hash & (MASK)) | 1 << (hash2 & MASK);
        let bloom_idx = (hash / u64::BITS) & (self.bloom_filters.len() as u32 - 1);
        let bitmask_word = self.bloom_filters[bloom_idx as usize];
        (bitmask_word & bitmask) == bitmask
    }

    pub fn get_hashed(&self, name: &CStr, hash: u32) -> Option<&'a Sym> {
        self.may_contain(hash)
            .then(|| self.lookup(name, hash))
            .flatten()
    }

    pub fn get(&self, name: &CStr) -> Option<&'a Sym> {
        self.get_hashed(name, Self::hash(name.to_bytes()))
    }
}

pub struct Tls {
    chunk_layout: Layout,
    init_data: NonNull<[u8]>,
    layout: Layout,
    data: NonNull<u8>,
}

impl Tls {
    pub fn new(init_data: NonNull<[u8]>, layout: Layout) -> Result<Self, AllocError> {
        let data = Global.allocate_zeroed(layout)?.as_non_null_ptr();
        unsafe {
            data.as_ptr()
                .copy_from_nonoverlapping(init_data.as_mut_ptr(), init_data.len())
        }
        log::debug!("TLS at: {:p}", data);
        Ok(Tls {
            init_data,
            chunk_layout: layout,
            layout,
            data,
        })
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.data.as_ptr()
    }

    pub fn chunk_layout(&self) -> Layout {
        self.chunk_layout
    }

    pub fn static_base(&self) -> *mut u8 {
        unsafe {
            let ptr = self.data.as_ptr();
            let size = self.chunk_layout.size();
            ptr.add(size)
        }
    }

    pub fn add_new(&mut self) -> usize {
        let (layout, offset) = self.layout.extend(self.chunk_layout).unwrap();
        let data = unsafe { Global.grow_zeroed(self.data, self.layout, layout).unwrap() }
            .as_non_null_ptr();
        unsafe {
            data.as_ptr()
                .add(offset)
                .copy_from_nonoverlapping(self.init_data.as_mut_ptr(), self.init_data.len())
        }
        self.layout = layout;
        self.data = data;
        offset / self.chunk_layout.pad_to_align().size()
    }

    pub fn get(&self, index: usize) -> Option<&[u8]> {
        let size = self.chunk_layout.size();
        let start = self.chunk_layout.pad_to_align().size().checked_mul(index)?;
        (size + start <= self.layout.size())
            .then(|| unsafe { slice::from_raw_parts(self.data.as_ptr().add(start), size) })
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut [u8]> {
        let size = self.chunk_layout.size();
        let start = self.chunk_layout.pad_to_align().size().checked_mul(index)?;
        (size + start <= self.layout.size())
            .then(|| unsafe { slice::from_raw_parts_mut(self.data.as_ptr().add(start), size) })
    }
}

impl Drop for Tls {
    fn drop(&mut self) {
        unsafe { Global.deallocate(self.data, self.layout) };
    }
}

#[repr(C)]
pub struct Tcb {
    pub static_base: *mut u8,
    pub index: usize,
}

impl Tcb {
    /// # Safety
    ///
    /// The caller must ensure that the current TCB is not present.
    pub unsafe fn init_current(index: usize) -> &'static mut Tcb {
        let mem = Box::new(Tcb {
            static_base: ptr::null_mut(),
            index,
        });
        let ret = Box::leak(mem);
        crate::arch::set_tls_reg(ret as *mut Tcb as u64);
        ret
    }
    /// # Safety
    ///
    /// The caller must ensure that the current TCB is present.
    pub unsafe fn current() -> &'static mut Tcb {
        let value = crate::arch::get_tls_reg();
        &mut *(value as *mut Tcb)
    }
}
