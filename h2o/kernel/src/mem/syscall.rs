use alloc::sync::Arc;
use core::{alloc::Layout, ptr::NonNull};

use bitop_ex::BitOpEx;
use sv_call::{mem::{MapInfo, MemInfo}, *};

use super::space;
use crate::{
    dev::Resource,
    sched::{task::Space as TaskSpace, Arsc, PREEMPT, SCHED},
    syscall::{In, Out, UserPtr},
};

fn check_layout(size: usize, align: usize) -> Result<Layout> {
    if size.contains_bit(paging::PAGE_MASK) || !align.is_power_of_two() {
        return Err(Error::EINVAL);
    }
    Layout::from_size_align(size, align).map_err(Error::from)
}

fn check_flags(flags: u32) -> Result<space::Flags> {
    let flags = space::Flags::from_bits(flags).ok_or(Error::EINVAL)?;
    if !flags.contains(space::Flags::USER_ACCESS) {
        return Err(Error::EPERM);
    }
    Ok(flags)
}

#[syscall]
fn phys_alloc(size: usize, align: usize, flags: u32) -> Result<Handle> {
    let layout = check_layout(size, align)?;
    let flags = check_flags(flags)?;
    let phys = PREEMPT.scope(|| space::Phys::allocate(layout, flags))?;
    SCHED.with_current(|cur| cur.space().handles().insert(phys))
}

#[syscall]
fn mem_map(space: Handle, mi: UserPtr<In, MapInfo>) -> Result<*mut u8> {
    let mi = unsafe { mi.read() }?;
    let flags = check_flags(mi.flags)?;
    let phys = SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<Arsc<space::Phys>>(mi.phys)
            .map(|obj| Arsc::clone(obj))
    })?;
    let op = |space: &Arsc<space::Space>| {
        let offset = if mi.map_addr {
            Some(
                mi.addr
                    .checked_sub(space.range.start)
                    .ok_or(Error::ERANGE)?,
            )
        } else {
            None
        };
        space
            .map(offset, phys, mi.phys_offset, mi.len, flags)
            .map(|addr| *addr)
    };
    if space == Handle::NULL {
        space::with_current(op)
    } else {
        SCHED.with_current(|cur| op(cur.space().handles().get::<Arsc<TaskSpace>>(space)?.mem()))
    }
}

#[syscall]
fn mem_reprot(space: Handle, ptr: *mut u8, len: usize, flags: u32) -> Result {
    let flags = check_flags(flags)?;
    unsafe {
        let ptr = NonNull::new(ptr).ok_or(Error::EINVAL)?;
        let ptr = NonNull::slice_from_raw_parts(ptr, len);
        if space == Handle::NULL {
            space::with_current(|cur| cur.reprotect(ptr, flags))
        } else {
            SCHED.with_current(|cur| {
                cur.space()
                    .handles()
                    .get::<Arsc<TaskSpace>>(space)?
                    .mem()
                    .reprotect(ptr, flags)
            })
        }
    }
}

#[syscall]
fn mem_unmap(space: Handle, ptr: *mut u8) -> Result {
    unsafe {
        let ptr = NonNull::new(ptr).ok_or(Error::EINVAL)?;
        if space == Handle::NULL {
            space::with_current(|cur| cur.unmap(ptr))
        } else {
            SCHED.with_current(|cur| {
                cur.space()
                    .handles()
                    .get::<Arsc<TaskSpace>>(space)?
                    .mem()
                    .unmap(ptr)
            })
        }
    }
}

#[syscall]
fn mem_info(info: UserPtr<Out, MemInfo>) -> Result {
    info.check()?;
    let all_available = super::ALL_AVAILABLE.load(core::sync::atomic::Ordering::Relaxed);
    let current_used = super::heap::current_used();
    unsafe {
        info.write(MemInfo {
            all_available,
            current_used,
        })
    }
}

#[syscall]
fn phys_acq(res: Handle, addr: usize, size: usize, align: usize, flags: u32) -> Result<Handle> {
    if addr.contains_bit(paging::PAGE_MASK)
        || size.contains_bit(paging::PAGE_MASK)
        || !align.is_power_of_two()
        || align.contains_bit(paging::PAGE_MASK)
    {
        return Err(Error::EINVAL);
    }
    let flags = check_flags(flags)?;

    SCHED.with_current(|cur| {
        let res = cur.space().handles().get::<Arc<Resource<usize>>>(res)?;
        if res.magic_eq(super::mem_resource())
            && res.range().start <= addr
            && addr + size <= res.range().end
        {
            let align = paging::PAGE_LAYOUT.align();
            let layout = unsafe { Layout::from_size_align(size, align) }?;
            let phys = space::Phys::new(paging::PAddr::new(addr), layout, flags);
            cur.space().handles().insert(phys)
        } else {
            Err(Error::EPERM)
        }
    })
}
