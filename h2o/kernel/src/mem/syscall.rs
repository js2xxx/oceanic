use alloc::sync::Arc;
use core::{alloc::Layout, ptr::NonNull, slice};

use bitop_ex::BitOpEx;
use sv_call::{
    mem::{MapInfo, MemInfo},
    *,
};

use super::space::{self, Flags};
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

fn check_flags(flags: Flags) -> Result<Flags> {
    if !flags.contains(Flags::USER_ACCESS) {
        return Err(Error::EPERM);
    }
    Ok(flags)
}

#[syscall]
fn phys_alloc(size: usize, align: usize, flags: Flags) -> Result<Handle> {
    let layout = check_layout(size, align)?;
    let flags = check_flags(flags)?;
    let phys = PREEMPT.scope(|| space::Phys::allocate(layout, flags))?;
    SCHED.with_current(|cur| cur.space().handles().insert_shared(phys))
}

fn phys_rw_check<T: crate::syscall::Type>(
    hdl: Handle,
    offset: usize,
    len: usize,
    buffer: UserPtr<T, u8>,
) -> Result<Arsc<space::Phys>> {
    hdl.check_null()?;
    buffer.check_slice(len)?;
    let offset_end = offset.wrapping_add(len);
    if offset_end < offset {
        return Err(Error::ERANGE);
    }
    let phys = SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<Arsc<space::Phys>>(hdl)
            .map(|obj| Arsc::clone(obj))
    })?;
    if offset_end > phys.layout().size() {
        return Err(Error::ERANGE);
    }
    Ok(phys)
}

#[syscall]
fn phys_read(hdl: Handle, offset: usize, len: usize, buffer: UserPtr<Out, u8>) -> Result {
    let phys = phys_rw_check(hdl, offset, len, buffer)?;
    if !phys.flags().contains(Flags::READABLE) {
        return Err(Error::EPERM);
    }
    if len > 0 {
        unsafe {
            let ptr = phys.raw_ptr().add(offset);
            let slice = slice::from_raw_parts(ptr, len);
            buffer.write_slice(slice)?;
        }
    }
    Ok(())
}

#[syscall]
fn phys_write(hdl: Handle, offset: usize, len: usize, buffer: UserPtr<In, u8>) -> Result {
    let phys = phys_rw_check(hdl, offset, len, buffer)?;
    if !phys.flags().contains(Flags::WRITABLE) {
        return Err(Error::EPERM);
    }
    if len > 0 {
        unsafe {
            let ptr = phys.raw_ptr().add(offset);
            buffer.read_slice(ptr, len)?;
        }
    }
    Ok(())
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
fn mem_reprot(space: Handle, ptr: *mut u8, len: usize, flags: Flags) -> Result {
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
fn phys_acq(res: Handle, addr: usize, size: usize, align: usize, flags: Flags) -> Result<Handle> {
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
            cur.space().handles().insert_shared(phys)
        } else {
            Err(Error::EPERM)
        }
    })
}
