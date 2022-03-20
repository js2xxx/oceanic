use core::sync::atomic::{self, Ordering::SeqCst};

use object::{elf::*, Endianness};
use solvent::prelude::Handle;

use crate::{elf::apply_relr, DT_RELR, DT_RELRSZ};

#[panic_handler]
fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
    log::error!("{}", info);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

/// The function indicating memory runs out.
#[alloc_error_handler]
fn rust_oom(layout: core::alloc::Layout) -> ! {
    log::error!("Allocation error for {:?}", layout);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

static mut BASE: usize = 0;
pub fn load_address() -> usize {
    unsafe { BASE }
}

static mut DYN: usize = 0;
pub fn dynamic() -> usize {
    unsafe { DYN }
}

#[no_mangle]
#[naked]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::asm!(
        "
        .weak   __ehdr_start
        .hidden __ehdr_start
        .weak   _DYNAMIC
        .hidden _DYNAMIC

        and rsp, ~0xF
        xor rbp, rbp
        lea rdx, [rip + __ehdr_start]
        lea rcx, [rip + _DYNAMIC]
        call {dl_start}
        mov rdi, rax
        push rbp
        jmp rdx",
        dl_start = sym dl_start,
        options(noreturn)
    )
}

#[repr(C)]
pub struct DlReturn {
    pub arg: usize,
    pub entry: usize,
}

unsafe extern "C" fn dl_start(
    init_chan: Handle,
    vdso_map: *mut u8,
    base_addr: usize,
    dynamic_addr: usize,
) -> DlReturn {
    assert!(init_chan != Handle::NULL);

    let base = base_addr as *mut FileHeader64<Endianness>;
    assert!((*base).e_ident.magic == ELFMAG);
    let endian = if (*base).e_ident.data == ELFDATA2MSB {
        Endianness::Big
    } else {
        Endianness::Little
    };

    let mut dynamic = dynamic_addr as *mut Dyn64<Endianness>;
    let (mut rel, mut crel) = (None, None);
    let (mut rela, mut crela) = (None, None);
    let (mut relr, mut crelr) = (None, None);

    while (*dynamic).d_tag.get(endian) != DT_NULL.into() {
        let d_tag = (*dynamic).d_tag.get(endian) as u32;
        let d_val = (*dynamic).d_val.get(endian) as usize;
        match d_tag {
            DT_REL => rel = Some((base as *mut u8).add(d_val)),
            DT_RELCOUNT => crel = Some(d_val),
            DT_RELA => rela = Some((base as *mut u8).add(d_val)),
            DT_RELACOUNT => crela = Some(d_val),
            DT_RELR => relr = Some((base as *mut u8).add(d_val)),
            DT_RELRSZ => crelr = Some(d_val),
            _ => {}
        }
        dynamic = dynamic.add(1);
    }

    if let (Some(rel), Some(len)) = (rel, crel) {
        let rel = rel.cast::<Rel64<Endianness>>();
        for i in 0..len {
            let offset = (*rel.add(i)).r_offset.get(endian) as usize;
            let ptr = (base as *mut u8).add(offset).cast::<usize>();
            *ptr += base as usize;
        }
    }

    if let (Some(rela), Some(len)) = (rela, crela) {
        let rela = rela.cast::<Rela64<Endianness>>();
        for i in 0..len {
            let offset = (*rela.add(i)).r_offset.get(endian) as usize;
            let addend = (*rela.add(i)).r_addend.get(endian) as usize;
            let ptr = (base as *mut u8).add(offset).cast::<usize>();
            *ptr += base as usize + addend;
        }
    }

    if let (Some(relr), Some(len)) = (relr, crelr) {
        apply_relr(base.cast(), relr.cast(), len);
    }

    atomic::fence(SeqCst);

    BASE = base_addr;
    DYN = dynamic_addr;
    crate::dl_main(init_chan, vdso_map)
}
