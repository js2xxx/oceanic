use core::{
    mem,
    sync::atomic::{self, Ordering::SeqCst},
};

use solvent::prelude::{Channel, Handle, Object};

use crate::elf::*;

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

pub fn load_address() -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!(
            "
        .weak   __ehdr_start
        .hidden __ehdr_start
        lea {}, [rip + __ehdr_start]",
            out(reg) ret
        );
    }
    ret
}

pub fn dynamic() -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!(
            "
        .weak   _DYNAMIC
        .hidden _DYNAMIC
        lea {}, [rip + _DYNAMIC]",
            out(reg) ret
        );
    }
    ret
}

static mut _VDSO_MAP: usize = 0;
pub fn vdso_map() -> usize {
    unsafe { _VDSO_MAP }
}

#[no_mangle]
#[naked]
unsafe extern "C" fn _start() -> ! {
    core::arch::asm!(
        "
        and rsp, ~0xF
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
    pub init_chan: Handle,
    pub entry: usize,
}

unsafe extern "C" fn dl_start(init_chan: Handle, vdso_map: usize) -> DlReturn {
    assert!(init_chan != Handle::NULL);

    let base = load_address() as *mut Header;
    assert!(&(*base).e_ident[..SELFMAG] == ELFMAG);
    assert!((*base).e_ident[EI_DATA] == ELFDATA2LSB);

    let mut dynamic = dynamic() as *mut Dyn;
    let (mut rel, mut crel) = (None, None);
    let (mut rela, mut crela) = (None, None);
    let (mut relr, mut szrelr) = (None, None);

    while (*dynamic).d_tag != DT_NULL {
        let d_tag = (*dynamic).d_tag;
        let d_val = (*dynamic).d_val as usize;
        match d_tag {
            DT_REL => rel = Some((base as *mut u8).add(d_val)),
            DT_RELCOUNT => crel = Some(d_val),
            DT_RELSZ => crel = crel.or(Some(d_val / mem::size_of::<Rel>())),
            DT_RELA => rela = Some((base as *mut u8).add(d_val)),
            DT_RELACOUNT => crela = Some(d_val),
            DT_RELASZ => crela = crela.or(Some(d_val / mem::size_of::<Rela>())),
            DT_RELR => relr = Some((base as *mut u8).add(d_val)),
            DT_RELRSZ => szrelr = Some(d_val),
            _ => {}
        }
        dynamic = dynamic.add(1);
    }

    if let (Some(rel), Some(len)) = (rel, crel) {
        let rel = rel.cast::<Rel>();
        for i in 0..len {
            let offset = (*rel.add(i)).r_offset as usize;
            let ptr = (base as *mut u8).add(offset).cast::<usize>();
            *ptr += base as usize;
        }
    }

    if let (Some(rela), Some(len)) = (rela, crela) {
        let rela = rela.cast::<Rela>();
        for i in 0..len {
            let offset = (*rela.add(i)).r_offset as usize;
            let addend = (*rela.add(i)).r_addend as usize;
            let ptr = (base as *mut u8).add(offset).cast::<usize>();
            *ptr += base as usize + addend;
        }
    }

    if let (Some(relr), Some(size)) = (relr, szrelr) {
        apply_relr(base.cast(), relr.cast(), size);
    }

    atomic::fence(SeqCst);
    _VDSO_MAP = vdso_map;

    crate::dl_main(Channel::from_raw(init_chan))
}
