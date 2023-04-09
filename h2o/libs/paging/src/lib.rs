#![no_std]
#![feature(step_trait)]

mod addr;
mod alloc;
mod consts;
mod entry;
mod inner;
mod level;

use core::{ops::Range, ptr::NonNull};

pub use self::{
    addr::{LAddr, PAddr},
    alloc::PageAlloc,
    consts::*,
    entry::{Attr, Entry, Table},
    level::Level,
};

pub const PAGE_LAYOUT: core::alloc::Layout = core::alloc::Layout::new::<Table>();

#[derive(Clone, Debug)]
pub struct MapInfo {
    pub virt: Range<LAddr>,
    pub phys: PAddr,
    pub attr: Attr,
    pub id_off: usize,
}

impl MapInfo {
    fn advance(&mut self, offset: usize) {
        self.virt.start.advance(offset);
        self.phys = PAddr::new(*self.phys + offset);
    }

    fn distance(&self, other: &MapInfo) -> Range<LAddr> {
        self.virt.start..other.virt.start
    }
}

#[derive(Clone, Debug)]
pub struct ReprotectInfo {
    pub virt: Range<LAddr>,
    pub attr: Attr,
    pub id_off: usize,
}

impl ReprotectInfo {
    fn advance(&mut self, offset: usize) {
        self.virt.start.advance(offset);
    }
}

pub fn maps(
    root_table: &mut Table,
    info: &MapInfo,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    log::trace!(
        "paging::maps: root table = {:?}, info = {:?}, allocator = {:?}",
        root_table as *mut _,
        info,
        allocator as *mut _
    );

    inner::check(&info.virt, Some(info.phys))?;

    let mut ret = Ok(());
    let mut rem_info = info.clone();
    log::trace!("paging::maps: Begin spliting pages");
    while !rem_info.virt.is_empty() {
        let level = Level::fit_all(&rem_info.virt, rem_info.phys);

        ret = inner::new_page(
            root_table,
            rem_info.virt.start,
            rem_info.phys,
            info.attr,
            level,
            info.id_off,
            allocator,
        );
        if ret.is_err() {
            break;
        }

        let ps = level.page_size();
        rem_info.advance(ps);

        log::trace!("paging::maps: Done new_page. rem = {:?}", &rem_info);
    }

    if ret.is_ok() {
        log::trace!("paging::maps: mapping succeeded");
    } else {
        let done_virt = info.distance(&rem_info);
        if !done_virt.is_empty() {
            let _ = unmaps(root_table, done_virt, info.id_off, allocator);
        }
    }
    ret
}

pub fn reprotect(
    root_table: &mut Table,
    info: &ReprotectInfo,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    log::trace!(
        "paging::reprotect: root table = {:?}, info = {:?}, allocator = {:?}",
        root_table as *mut _,
        info,
        allocator as *mut _
    );

    inner::check(&info.virt, None)?;

    let mut rem_info = info.clone();
    while !rem_info.virt.is_empty() {
        let phys = query(root_table, rem_info.virt.start, rem_info.id_off)
            .map_or_else(|_| PAddr::new(0), |(phys, _)| phys);
        let level = Level::fit_all(&rem_info.virt, phys);

        match inner::modify_page(
            root_table,
            rem_info.virt.start,
            rem_info.attr,
            level,
            rem_info.id_off,
            allocator,
        ) {
            Ok(()) | Err(Error::EntryExistent(false)) => {}
            Err(e) => panic!("{:?}", e),
        }

        let ps = level.page_size();
        rem_info.advance(ps);
    }

    Ok(())
}

pub fn query(root_table: &Table, virt: LAddr, id_off: usize) -> Result<(PAddr, Attr), Error> {
    inner::get_page(root_table, virt, id_off)
}

pub fn unmaps(
    root_table: &mut Table,
    mut virt: Range<LAddr>,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    log::trace!(
        "paging::unmaps: root table = {:?}, virt = {:?}, id_off = {:?}, allocator = {:?}",
        root_table as *mut _,
        virt,
        id_off,
        allocator as *mut _
    );

    inner::check(&virt, None)?;

    while !virt.is_empty() {
        let phys =
            query(root_table, virt.start, id_off).map_or_else(|_| PAddr::new(0), |(phys, _)| phys);
        let level = Level::fit_all(&virt, phys);

        let _ = inner::drop_page(root_table, virt.start, level, id_off, allocator);

        let ps = level.page_size();
        virt.start.advance(ps);
    }

    Ok(())
}
