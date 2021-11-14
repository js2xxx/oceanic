use alloc::{alloc::Global, collections::BTreeMap};
use core::{
    alloc::{Allocator, Layout},
    mem,
    ops::Range,
    ptr::{null_mut, NonNull},
};

use paging::{LAddr, PAddr};
use spin::Mutex;

use super::*;

const PHYS_RANGE: Range<PAddr> = PAddr::new(0)..PAddr::new(minfo::INITIAL_ID_SPACE);
const VIRT_RANGE: Range<LAddr> = LAddr::new(minfo::ID_OFFSET as *mut u8)
    ..LAddr::new((minfo::ID_OFFSET + minfo::INITIAL_ID_SPACE) as *mut u8);

static ALLOC_REC: Mutex<BTreeMap<usize, Layout>> = Mutex::new(BTreeMap::new());

#[no_mangle]
unsafe extern "C" fn AcpiOsMapMemory(paddr: ACPI_PHYSICAL_ADDRESS, len: ACPI_SIZE) -> *mut u8 {
    let paddr = PAddr::new(paddr as usize);
    assert!(PHYS_RANGE.contains(&paddr));

    *paddr.to_laddr(minfo::ID_OFFSET)
}

#[no_mangle]
unsafe extern "C" fn AcpiOsUnmapMemory(_laddr: *mut u8, _len: ACPI_SIZE) {}

#[no_mangle]
unsafe extern "C" fn AcpiOsGetPhysicalAddress(
    laddr: *mut u8,
    paddr: *mut ACPI_PHYSICAL_ADDRESS,
) -> ACPI_STATUS {
    let laddr = LAddr::new(laddr);

    if laddr.is_null() || !paddr.is_null() {
        AE_BAD_PARAMETER
    } else if VIRT_RANGE.contains(&laddr) {
        *paddr = *laddr.to_paddr(minfo::ID_OFFSET) as ACPI_PHYSICAL_ADDRESS;
        AE_OK
    } else {
        AE_ERROR
    }
}

#[no_mangle]
unsafe extern "C" fn AcpiOsReadMemory(
    paddr: ACPI_PHYSICAL_ADDRESS,
    val: *mut UINT64,
    width: UINT32,
) -> ACPI_STATUS {
    unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsWriteMemory(
    paddr: ACPI_PHYSICAL_ADDRESS,
    val: UINT64,
    width: UINT32,
) -> ACPI_STATUS {
    unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsAllocate(size: ACPI_SIZE) -> *mut u8 {
    let size = size as usize;
    Layout::from_size_align(size, size.next_power_of_two())
        .ok()
        .and_then(|layout| {
            Global.allocate(layout).map_or(None, |ptr| {
                let ptr = ptr.as_mut_ptr();
                if !ptr.is_null() {
                    let mut rec = ALLOC_REC.lock();
                    rec.insert(ptr as usize, layout);
                }
                Some(ptr)
            })
        })
        .unwrap_or(null_mut())
}

#[no_mangle]
unsafe extern "C" fn AcpiOsFree(ptr: *mut u8) {
    let layout = {
        let mut rec = ALLOC_REC.lock();
        rec.remove(&(ptr as usize))
    };

    if let Some((layout, ptr)) = layout.zip(NonNull::new(ptr)) {
        Global.deallocate(ptr, layout);
    }
}

#[no_mangle]
unsafe extern "C" fn strlen(ptr: *const u8) -> usize {
    if (ptr as usize) & (mem::size_of::<u32>() - 1) == 0 {
        let mut len = 0;
        let mut ptr = ptr as *const u32;
        loop {
            match (*ptr).to_ne_bytes() {
                [0, ..] => return len,
                [_, 0, ..] => return len + 1,
                [_, _, 0, ..] => return len + 2,
                [_, _, _, 0] => return len + 3,
                _ => {
                    len += mem::size_of::<u32>();
                    ptr = ptr.add(1);
                }
            }
        }
    } else {
        let mut len = 0;
        let mut ptr = ptr;
        while *ptr != 0 {
            len += 1;
            ptr = ptr.add(1);
        }
        len
    }
}

#[no_mangle]
unsafe extern "C" fn strncmp(a: *const u8, b: *const u8, num: c_int) -> c_int {
    let la = strlen(a);
    let lb = strlen(b);
    {
        let n = (num as usize).min(la).min(lb);
        for i in 0..n {
            if *a.add(i) < *b.add(i) {
                return -(i as i32);
            } else if *a.add(i) > *b.add(i) {
                return (i as i32);
            }
        }
        return 0;
    }
}

#[no_mangle]
unsafe extern "C" fn isdigit(c: c_int) -> c_int {
    (c as u8).is_ascii_digit() as c_int
}

#[no_mangle]
unsafe extern "C" fn isprint(c: c_int) -> c_int {
    !(c as u8).is_ascii_control() as c_int
}
