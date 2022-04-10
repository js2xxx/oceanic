use alloc::sync::{Arc, Weak};
use core::{alloc::Layout, slice};

use bitop_ex::BitOpEx;
use paging::LAddr;
use sv_call::{
    mem::{Flags, MemInfo, VirtMapInfo},
    *,
};

use super::space;
use crate::{
    dev::Resource,
    sched::{
        task::{hdl::DefaultFeature, Space as TaskSpace, VDSO},
        PREEMPT, SCHED,
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
            .map(|obj| (obj.features(), space::Phys::clone(&obj)))
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
    if !feat.contains(Feature::READ) {
        return Err(Error::EPERM);
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
fn space_new(root_virt: UserPtr<Out, Handle>) -> Result<Handle> {
    root_virt.check()?;
    SCHED.with_current(|cur| {
        let space = TaskSpace::new(cur.tid().ty())?;
        let virt = Arc::downgrade(space.mem().root());
        let ret = cur.space().handles().insert(space, None)?;
        unsafe {
            let virt = cur.space().handles().insert_unchecked(
                virt,
                Weak::<space::Virt>::default_features() | Feature::SEND,
                None,
            )?;
            root_virt.write(virt)?;
        }
        Ok(ret)
    })
}

#[syscall]
fn virt_alloc(hdl: Handle, offset: usize, size: usize, align: usize) -> Result<Handle> {
    hdl.check_null()?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(Error::EKILLED)?;
        let sub = virt.allocate(
            (offset != usize::MAX).then(|| offset),
            Layout::from_size_align(size, align)?,
        )?;
        cur.space().handles().insert(sub, None)
    })
}

#[syscall]
fn virt_info(hdl: Handle, size: UserPtr<Out, usize>) -> Result<*mut u8> {
    hdl.check_null()?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(Error::EKILLED)?;
        let base = virt.range().start;
        if !size.as_ptr().is_null() {
            unsafe { size.write(virt.len()) }?;
        }
        Ok(*base)
    })
}

#[syscall]
fn virt_drop(hdl: Handle) -> Result {
    hdl.check_null()?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(Error::EKILLED)?;
        virt.destroy()
    })
}

#[syscall]
fn virt_map(hdl: Handle, mi: UserPtr<In, VirtMapInfo>) -> Result<*mut u8> {
    hdl.check_null()?;
    let mi = unsafe { mi.read() }?;
    let flags = check_flags(mi.flags)?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(Error::EKILLED)?;
        let phys = cur.space().handles().remove::<space::Phys>(mi.phys)?;
        let offset = (mi.offset != usize::MAX).then(|| mi.offset);
        if flags.intersects(!features_to_flags(phys.features())) {
            return Err(Error::EPERM);
        }

        let addr = virt.map(
            offset,
            space::Phys::clone(&phys),
            mi.phys_offset,
            space::page_aligned(mi.len),
            flags,
        )?;
        Ok(*addr)
    })
}

#[syscall]
fn virt_reprot(hdl: Handle, base: UserPtr<In, u8>, len: usize, flags: Flags) -> Result {
    hdl.check_null()?;
    base.check()?;
    let flags = check_flags(flags)?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(Error::EKILLED)?;
        virt.reprotect(LAddr::new(base.as_ptr()), len, flags)
    })
}

#[syscall]
fn virt_unmap(hdl: Handle, base: UserPtr<In, u8>, len: usize, drop_child: bool) -> Result {
    hdl.check_null()?;
    base.check()?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(Error::EKILLED)?;
        virt.unmap(LAddr::new(base.as_ptr()), len, drop_child)
    })
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
