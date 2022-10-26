use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{alloc::Layout, ptr};

use bitop_ex::BitOpEx;
use paging::LAddr;
use sv_call::{
    mem::{Flags, IoVec, MemInfo, PhysOptions, VirtMapInfo},
    *,
};

use super::space;
use crate::{
    dev::Resource,
    mem::space::PhysTrait,
    sched::{
        task::{
            hdl::{DefaultFeature, Ref},
            Space as TaskSpace, VDSO,
        },
        PREEMPT, SCHED,
    },
    syscall::{In, InOut, Out, PtrType, UserPtr},
};

fn check_flags(flags: Flags) -> Result<Flags> {
    if !flags.contains(Flags::USER_ACCESS) {
        return Err(EPERM);
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
fn phys_alloc(size: usize, options: PhysOptions) -> Result<Handle> {
    let phys = PREEMPT.scope(|| space::allocate_phys(size, options, false))?;
    SCHED.with_current(|cur| {
        let event = phys.event();
        cur.space().handles().insert_raw(phys, Some(event))
    })
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

fn phys_check(hdl: Handle, offset: usize, len: usize) -> Result<(Feature, Arc<space::Phys>)> {
    hdl.check_null()?;
    let offset_end = offset.wrapping_add(len);
    if offset_end < offset {
        return Err(ERANGE);
    }
    let (feat, phys) = SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<space::Phys>(hdl)
            .map(|obj| (obj.features(), Arc::clone(&obj)))
    })?;
    if phys == VDSO.1 {
        return Err(EACCES);
    }
    Ok((feat, phys))
}

#[syscall]
fn phys_read(hdl: Handle, offset: usize, len: usize, buffer: UserPtr<Out>) -> Result<usize> {
    buffer.check_slice(len)?;
    let (feat, phys) = phys_check(hdl, offset, len)?;
    if !feat.contains(Feature::READ) {
        return Err(EPERM);
    }
    if len > 0 {
        phys.read(offset, len, buffer)
    } else {
        Ok(0)
    }
}

#[syscall]
fn phys_write(hdl: Handle, offset: usize, len: usize, buffer: UserPtr<In>) -> Result<usize> {
    buffer.check_slice(len)?;
    let (feat, phys) = phys_check(hdl, offset, len)?;
    if !feat.contains(Feature::WRITE) {
        return Err(EPERM);
    }
    if len > 0 {
        phys.write(offset, len, buffer)
    } else {
        Ok(0)
    }
}

static_assertions::const_assert!({
    let a = Layout::new::<IoVec>();
    let b = Layout::new::<(UserPtr<In>, usize)>();
    a.size() == b.size() && a.align() == b.align()
});

#[allow(clippy::type_complexity)]
fn check_physv<T: PtrType>(
    hdl: Handle,
    bufs: UserPtr<In, IoVec>,
    count: usize,
) -> Result<(Feature, Arc<space::Phys>, Vec<(UserPtr<T>, usize)>)> {
    hdl.check_null()?;
    bufs.check_slice(count)?;
    let (feat, phys) = SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<space::Phys>(hdl)
            .map(|obj| (obj.features(), Arc::clone(&obj)))
    })?;
    let bufs = {
        let mut vec = Vec::<(UserPtr<T>, usize)>::with_capacity(count);
        let mem = vec.spare_capacity_mut();
        unsafe {
            bufs.read_slice(mem.as_mut_ptr() as _, count)?;
            vec.set_len(count);
        }
        vec
    };
    Ok((feat, phys, bufs))
}

#[syscall]
fn phys_readv(hdl: Handle, offset: usize, bufs: UserPtr<In, IoVec>, count: usize) -> Result<usize> {
    let (feat, phys, bufs) = check_physv(hdl, bufs, count)?;
    if !feat.contains(Feature::READ) {
        return Err(EPERM);
    }
    phys.read_vectored(offset, &bufs)
}

#[syscall]
fn phys_writev(
    hdl: Handle,
    offset: usize,
    bufs: UserPtr<In, IoVec>,
    count: usize,
) -> Result<usize> {
    let (feat, phys, bufs) = check_physv(hdl, bufs, count)?;
    if !feat.contains(Feature::WRITE) {
        return Err(EPERM);
    }
    phys.write_vectored(offset, &bufs)
}

#[syscall]
fn phys_sub(hdl: Handle, offset: usize, len: usize, copy: bool) -> Result<Handle> {
    let (feat, phys) = phys_check(hdl, offset, len)?;
    if !feat.contains(Feature::READ) {
        return Err(EPERM);
    }

    let sub = phys.create_sub(offset, len, copy)?;
    SCHED.with_current(|cur| {
        let handles = cur.space().handles();
        let event = sub.event();
        if copy {
            handles.insert_raw(sub, Some(event))
        } else {
            unsafe { handles.insert_raw_unchecked(sub, feat, Some(event)) }
        }
    })
}

#[syscall]
fn phys_resize(hdl: Handle, new_len: usize, zeroed: bool) -> Result {
    if new_len == 0 {
        return Err(EINVAL);
    }
    let (feat, phys) = SCHED.with_current(|cur| {
        cur.space()
            .handles()
            .get::<space::Phys>(hdl)
            .map(|obj| (obj.features(), Arc::clone(&obj)))
    })?;
    if phys == VDSO.1 {
        return Err(EACCES);
    }
    if !feat.contains(Feature::READ | Feature::WRITE | Feature::EXECUTE) {
        return Err(EPERM);
    }
    phys.resize(new_len, zeroed)
}

#[syscall]
fn space_new(root_virt: UserPtr<Out, Handle>) -> Result<Handle> {
    root_virt.check()?;
    SCHED.with_current(|cur| {
        let space = TaskSpace::new(cur.tid().ty())?;
        let virt = Arc::downgrade(space.mem().root());
        let ret = cur.space().handles().insert_raw(space, None)?;
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
        let virt = virt.upgrade().ok_or(EKILLED)?;
        let sub = virt.allocate(
            (offset != usize::MAX).then_some(offset),
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
        let virt = virt.upgrade().ok_or(EKILLED)?;
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
        let virt = virt.upgrade().ok_or(EKILLED)?;
        virt.destroy()
    })
}

#[syscall]
fn virt_map(hdl: Handle, mi_ptr: UserPtr<InOut, VirtMapInfo>) -> Result<*mut u8> {
    hdl.check_null()?;
    let mi = unsafe { mi_ptr.read() }?;
    let flags = check_flags(mi.flags)?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(EKILLED)?;
        let phys = cur.space().handles().remove::<space::Phys>(mi.phys)?;
        let offset = (mi.offset != usize::MAX).then_some(mi.offset);
        if flags.intersects(!features_to_flags(phys.features())) {
            return Err(EPERM);
        }

        let size = if mi.len == 0 { phys.len() } else { mi.len };
        let layout = Layout::from_size_align(size, mi.align)?;
        let addr = virt.map(offset, Ref::into_raw(phys), mi.phys_offset, layout, flags)?;
        unsafe {
            let len = UserPtr::<Out, _>::new(ptr::addr_of_mut!((*mi_ptr.as_ptr()).len));
            len.write(size)?;
        }
        Ok(*addr)
    })
}

#[syscall]
fn virt_reprot(hdl: Handle, base: UserPtr<In>, len: usize, flags: Flags) -> Result {
    hdl.check_null()?;
    base.check()?;
    let flags = check_flags(flags)?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(EKILLED)?;
        virt.reprotect(LAddr::new(base.as_ptr()), len, flags)
    })
}

#[syscall]
fn virt_unmap(hdl: Handle, base: UserPtr<In>, len: usize, drop_child: bool) -> Result {
    hdl.check_null()?;
    base.check()?;
    SCHED.with_current(|cur| {
        let virt = cur.space().handles().get::<Weak<space::Virt>>(hdl)?;
        let virt = virt.upgrade().ok_or(EKILLED)?;
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
        return Err(EINVAL);
    }

    SCHED.with_current(|cur| {
        let res = cur.space().handles().get::<Resource<usize>>(res)?;
        if !res.magic_eq(super::mem_resource()) {
            return Err(EPERM);
        }

        if addr == 0 {
            let phys = space::allocate_phys(size, PhysOptions::ZEROED, true)?;
            return unsafe { cur.space().handles().insert_raw(phys, None) };
        }

        if !(res.range().start <= addr && addr + size <= res.range().end) {
            return Err(EPERM);
        }
        let phys = space::new_phys(paging::PAddr::new(addr), size)?;
        unsafe { cur.space().handles().insert_raw(phys, None) }
    })
}
