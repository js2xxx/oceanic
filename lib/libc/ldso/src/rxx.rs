use core::sync::atomic::{self, Ordering::SeqCst};

use object::{elf, Endianness};
use solvent::prelude::Handle;

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

#[cold]
pub fn load_address() -> *mut elf::FileHeader64<Endianness> {
    extern "C" {
        static __ehdr_start: u8;
    }
    unsafe { &__ehdr_start as *const u8 as *mut _ }
}

#[cold]
pub fn dynamic_offset() -> usize {
    extern "C" {
        static _DYNAMIC: u8;
    }
    unsafe { &_DYNAMIC as *const u8 as usize }
}

fn dynamic() -> *mut elf::Dyn64<Endianness> {
    let base = load_address() as *mut u8;
    unsafe { base.add(dynamic_offset()) as *mut _ }
}

#[no_mangle]
#[naked]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::asm!(
        "and rsp, ~0xF
        xor rbp, rbp
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

unsafe extern "C" fn dl_start(init_chan: Handle, vdso_map: *mut u8) -> DlReturn {
    assert!(init_chan != Handle::NULL);

    let base = load_address();
    assert!((*base).e_ident.magic == elf::ELFMAG);
    let endian = if (*base).e_ident.data == elf::ELFDATA2MSB {
        Endianness::Big
    } else {
        Endianness::Little
    };

    let mut dynamic = dynamic();
    let (mut rel, mut crel) = (None, None);
    let (mut rela, mut crela) = (None, None);

    while (*dynamic).d_tag.get(endian) != elf::DT_NULL.into() {
        let d_tag = (*dynamic).d_tag.get(endian) as u32;
        let d_val = (*dynamic).d_val.get(endian) as usize;
        match d_tag {
            elf::DT_REL => rel = Some((base as *mut u8).add(d_val)),
            elf::DT_RELCOUNT => crel = Some(d_val),
            elf::DT_RELA => rela = Some((base as *mut u8).add(d_val)),
            elf::DT_RELACOUNT => crela = Some(d_val),
            _ => {}
        }
        dynamic = dynamic.add(1);
    }

    if let (Some(rel), Some(len)) = (rel, crel) {
        let rel = rel.cast::<elf::Rel64<Endianness>>();
        for i in 0..len {
            let offset = (*rel.add(i)).r_offset.get(endian) as usize;
            let ptr = (base as *mut u8).add(offset).cast::<usize>();
            *ptr += base as usize;
        }
    }

    if let (Some(rela), Some(len)) = (rela, crela) {
        let rela = rela.cast::<elf::Rela64<Endianness>>();
        for i in 0..len {
            let offset = (*rela.add(i)).r_offset.get(endian) as usize;
            let addend = (*rela.add(i)).r_addend.get(endian) as usize;
            let ptr = (base as *mut u8).add(offset).cast::<usize>();
            *ptr += base as usize + addend;
        }
    }

    atomic::fence(SeqCst);

    crate::dl_main(init_chan, vdso_map)
}
