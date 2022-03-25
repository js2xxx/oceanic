use alloc::sync::Arc;
use core::{ptr::NonNull, slice};

use bitop_ex::BitOpEx;
use sv_call::{
    mem::{Flags, MapInfo, MemInfo},
    *,
};

use super::space;
use crate::{
    dev::Resource,
    sched::{
        task::{Space as TaskSpace, VDSO},
        Arsc, PREEMPT, SCHED,
    },
    syscall::{In, Out, UserPtr},
};

fn check_flags(flags: Flags) -> Result<Flags> {
    if !flags.contains(Flags::USER_ACCESS) {
        return Err(Error::EPERM);
    }
    Ok(flags)
}

fn features_to_flags(feat: Feature) -> Flags {
    let mut flags = Flags::USER_ACCESS;
    if feat.contains(Feature::READ) {
        flags |= Flags::READABLE;
    }
    if feat.contains(Feature::WRITE) {
        flags |= Flags::WRITABLE;
    }
    if feat.contains(Feature::EXECUTE) {
        flags |= Flags::EXECUTABLE;
    }
    flags
}

#[syscall]
fn phys_alloc(size: usize, zeroed: bool) -> Result<Handle> {
    let phys = PREEMPT.scope(|| space::Phys::allocate(size, zeroed))?;
    SCHED.with_current(|cur| unsafe { cur.space().handles().insert(phys, None) })
}

#[syscall]
fn phys_size(hdl: Handle) -> Result<usize> {
    hdl.check_null()?;
    SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<space::Phys>(hdl)
            .map(|phys| phys.len())
    })
}

fn phys_check(hdl: Handle, offset: usize, len: usize) -> Result<(Feature, space::Phys)> {
    hdl.check_null()?;
    let offset_end = offset.wrapping_add(len);
    if offset_end < offset {
        return Err(Error::ERANGE);
    }
    let (feat, phys) = SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<space::Phys>(hdl)
            .map(|obj| (obj.features(), space::Phys::clone(obj)))
    })?;
    if offset_end > phys.len() {
        return Err(Error::ERANGE);
    }
    Ok((feat, phys))
}

#[syscall]
fn phys_read(hdl: Handle, offset: usize, len: usize, buffer: UserPtr<Out, u8>) -> Result {
    buffer.check_slice(len)?;
    let (feat, phys) = phys_check(hdl, offset, len)?;
    if phys == VDSO.1 {
        return Err(Error::EACCES);
    }
    if !feat.contains(Feature::READ) {
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
    buffer.check_slice(len)?;
    let (feat, phys) = phys_check(hdl, offset, len)?;
    if phys == VDSO.1 {
        return Err(Error::EACCES);
    }
    if !feat.contains(Feature::WRITE) {
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
fn phys_sub(hdl: Handle, offset: usize, len: usize, copy: bool) -> Result<Handle> {
    let (feat, phys) = phys_check(hdl, offset, len)?;
    if phys == VDSO.1 {
        return Err(Error::EACCES);
    }

    let sub = phys.create_sub(offset, len, copy)?;
    SCHED.with_current(|cur| {
        let handles = cur.space().handles();
        if copy {
            handles.insert(sub, None)
        } else {
            unsafe { handles.insert_unchecked(sub, feat, None) }
        }
    })
}

#[syscall]
fn mem_map(space: Handle, mi: UserPtr<In, MapInfo>) -> Result<*mut u8> {
    let mi = unsafe { mi.read() }?;
    let flags = check_flags(mi.flags)?;
    let (feat, phys) = SCHED.with_current(|cur| {
        let obj = cur.space().handles().remove::<space::Phys>(mi.phys)?;
        Ok((
            obj.features(),
            space::Phys::clone(obj.downcast_ref::<space::Phys>()?),
        ))
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
        if flags & !features_to_flags(feat) != Flags::empty() {
            return Err(Error::EPERM);
        }
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
fn mem_get(space: Handle, ptr: UserPtr<In, u8>, flags: UserPtr<Out, Flags>) -> Result<usize> {
    ptr.check()?;
    let ptr = NonNull::new(ptr.as_ptr()).ok_or(Error::EINVAL)?;
    let mut f = Flags::empty();
    unsafe {
        if space == Handle::NULL {
            space::with_current(|cur| cur.get(ptr, &mut f))
        } else {
            SCHED.with_current(|cur| {
                cur.space()
                    .handles()
                    .get::<Arsc<TaskSpace>>(space)?
                    .mem()
                    .get(ptr, &mut f)
            })
        }
    }
    .and_then(|addr| {
        unsafe { flags.write(f)? };
        Ok(*addr)
    })
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
fn phys_acq(res: Handle, addr: usize, size: usize) -> Result<Handle> {
    if addr.contains_bit(paging::PAGE_MASK) || size.contains_bit(paging::PAGE_MASK) {
        return Err(Error::EINVAL);
    }

    SCHED.with_current(|cur| {
        let res = cur.space().handles().get::<Arc<Resource<usize>>>(res)?;
        if res.magic_eq(super::mem_resource())
            && res.range().start <= addr
            && addr + size <= res.range().end
        {
            let phys = space::Phys::new(paging::PAddr::new(addr), size)?;
            unsafe { cur.space().handles().insert(phys, None) }
        } else {
            Err(Error::EPERM)
        }
    })
}
