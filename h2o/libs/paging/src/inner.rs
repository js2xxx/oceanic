use crate::*;

fn create_table(
    entry: &mut Entry,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<NonNull<[Entry]>, Error> {
    log::trace!(
            "paging::create_table: entry = {:?}(value = {:?}), level = {:?}, id_off = {:?}, allocator = {:?}",
            entry as *mut _,
            *entry,
            level, id_off,
            allocator as *mut _
      );

    assert!(level != Level::Pt, "Too low level");

    match entry.get_table(id_off, level) {
        Some(ptr) => Ok(ptr),
        None => {
            if entry.is_leaf(level) {
                Err(Error::EntryExistent(true))
            } else {
                let phys = unsafe { allocator.alloc_zeroed(id_off) }.ok_or(Error::OutOfMemory)?;
                let attr = Attr::INTERMEDIATE;
                *entry = Entry::new(phys, attr, Level::Pt);
                log::trace!(
                    "paging::create_table: allocated new table at phys {:?}",
                    phys
                );
                Ok(entry
                    .get_table(id_off, level)
                    .expect("Failed to get the table of the entry"))
            }
        }
    }
}

fn split_table(
    entry: &mut Entry,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    let (phys, mut attr) = entry.get(Level::Pt);
    entry.reset();
    attr &= !Attr::LARGE_PAT;

    let item_level = level.decrease().expect("Too low level");
    let item_attr = item_level.leaf_attr(attr);
    let mut table = create_table(entry, level, id_off, allocator)?;
    let table_mut = unsafe { table.as_mut() };
    for (i, item) in table_mut.iter_mut().enumerate() {
        let item_phys = PAddr::new(*phys + i * item_level.page_size());
        *item = Entry::new(item_phys, item_attr, item_level);
    }

    Ok(())
}

fn get_or_split_table(
    entry: &mut Entry,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<NonNull<[Entry]>, Error> {
    assert!(level != Level::Pt, "Too low level");

    entry.get_table(id_off, level).map_or_else(
        || {
            if entry.is_leaf(level) {
                split_table(entry, level, id_off, allocator)?;

                Ok(entry
                    .get_table(id_off, level)
                    .expect("Failed to get the table of the entry"))
            } else {
                Err(Error::EntryExistent(false))
            }
        },
        Ok,
    )
}

pub(crate) unsafe fn invalidate_page(virt: LAddr) {
    asm!("invlpg [{}]", in(reg) *virt);
}

pub(crate) fn new_page(
    root_table: &mut Table,
    virt: LAddr,
    phys: PAddr,
    attr: Attr,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    log::trace!(
            "paging::new_page: root table = {:?}, virt = {:?}, phys = {:?}, attr = {:?}, level = {:?}, id_off = {:?}, allocator = {:?}",
            root_table as *mut _,
            virt,
            phys,
            attr,
            level,
            id_off,
            allocator as *mut _
      );

    let mut table: NonNull<[Entry]> = NonNull::from(&mut **root_table);
    let mut lvl = Level::P4;
    while lvl != level {
        let idx = lvl.addr_idx(virt, false);
        let table_mut = unsafe { table.as_mut() };
        let item = &mut table_mut[idx];
        table = create_table(item, lvl, id_off, allocator)?;
        lvl = lvl.decrease().expect("Too low level");
    }

    let idx = level.addr_idx(virt, false);
    let table_mut = unsafe { table.as_mut() };

    if table_mut[idx].is_leaf(level) {
        Err(Error::EntryExistent(true))
    } else {
        let attr = level.leaf_attr(attr);
        table_mut[idx] = Entry::new(phys, attr, level);

        unsafe { invalidate_page(virt) };
        Ok(())
    }
}

pub(crate) fn modify_page(
    root_table: &mut Table,
    virt: LAddr,
    attr: Attr,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    let mut table: NonNull<[Entry]> = NonNull::from(&mut **root_table);
    let mut lvl = Level::P4;
    while lvl != level {
        let idx = lvl.addr_idx(virt, false);
        let table_mut = unsafe { table.as_mut() };
        let item = &mut table_mut[idx];
        table = get_or_split_table(item, lvl, id_off, allocator)?;
        lvl = lvl.decrease().expect("Too low level");
    }

    let idx = level.addr_idx(virt, false);
    let table_mut = unsafe { table.as_mut() };

    if table_mut[idx].is_leaf(level) {
        let attr = level.leaf_attr(attr);
        let (phys, _) = table_mut[idx].get(level);
        table_mut[idx] = Entry::new(phys, attr, level);

        unsafe { invalidate_page(virt) };
        Ok(())
    } else {
        Err(Error::EntryExistent(false))
    }
}

pub(crate) fn get_page(root_table: &Table, virt: LAddr, id_off: usize) -> Result<PAddr, Error> {
    let mut table: NonNull<[Entry]> = NonNull::from(&**root_table);
    let mut lvl = Level::P4;
    loop {
        let idx = lvl.addr_idx(virt, false);
        let table_ref = unsafe { table.as_ref() };
        let item = &table_ref[idx];

        if item.is_leaf(lvl) {
            break Ok(item.get(lvl).0);
        }

        table = item
            .get_table(id_off, lvl)
            .ok_or(Error::EntryExistent(false))?;
        lvl = lvl.decrease().ok_or(Error::EntryExistent(false))?;
    }
}

pub(crate) fn drop_page(
    root_table: &mut Table,
    virt: LAddr,
    level: Level,
    id_off: usize,
    allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
    let mut table: NonNull<[Entry]> = NonNull::from(&mut **root_table);
    let mut lvl = Level::P4;
    while lvl != level {
        let idx = lvl.addr_idx(virt, false);
        let table_mut = unsafe { table.as_mut() };
        let item = &mut table_mut[idx];
        table = get_or_split_table(item, lvl, id_off, allocator)?;
        lvl = lvl.decrease().expect("Too low level");
    }

    let idx = level.addr_idx(virt, false);
    let table_mut = unsafe { table.as_mut() };

    if table_mut[idx].is_leaf(level) {
        table_mut[idx].reset();

        unsafe { invalidate_page(virt) };
        Ok(())
    } else {
        Err(Error::EntryExistent(false))
    }
}

pub(crate) fn check(virt: &Range<LAddr>, phys: Option<PAddr>) -> Result<(), Error> {
    log::trace!("paging::check: virt = {:?}, phys = {:?}", virt, phys);

    #[inline]
    fn misaligned<Origin>(addr: usize, o: Origin) -> Option<Origin> {
        if addr & PAGE_MASK == 0 {
            None
        } else {
            log::warn!("paging::check: misaligned address: {:#x}", addr);
            Some(o)
        }
    }

    let (vstart, vend) = (virt.start.val(), virt.end.val());
    let ret = Error::AddrMisaligned {
        vstart: misaligned(vstart, virt.start),
        vend: misaligned(vend, virt.end),
        phys: phys.and_then(|phys| misaligned(*phys, phys)),
    };
    if !ret.is_misaligned_invalid() {
        return Err(ret);
    }

    if vstart >= vend {
        log::warn!("paging::check: linear address range is empty");
        return Err(Error::RangeEmpty);
    }

    Ok(())
}
