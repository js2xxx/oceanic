#![no_std]
#![feature(alloc_error_handler)]
#![feature(asm_sym)]
#![feature(naked_functions)]

mod rxx;

use solvent::prelude::*;

extern "C" {
    static _DYNAMIC: &'static [u8; 0];
    static __ehdr_start: &'static [u8; 0];
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

extern "C" fn dl_start(init_chan: Handle, vdso: *mut u8) -> DlReturn {
    unsafe {
        assert!(init_chan != Handle::NULL);
        assert!(__ehdr_start.as_ptr() != vdso);
        assert!(_DYNAMIC.as_ptr() != vdso);
    }
    todo!()
}
