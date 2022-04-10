use core::{alloc::Layout, ptr::NonNull};

use bootfs::parse::Directory;
use solvent::prelude::{Error as SError, Flags, Phys, Virt, PAGE_LAYOUT, PAGE_SIZE};
use sv_call::task::DEFAULT_STACK_SIZE;

const STACK_PROTECTOR_SIZE: usize = PAGE_SIZE;
const STACK_PROTECTOR_LAYOUT: Layout = PAGE_LAYOUT;

#[derive(Debug)]
pub enum Error {
    Solvent(SError),
    Load(elfload::Error),
}

impl From<SError> for Error {
    fn from(err: SError) -> Self {
        Error::Solvent(err)
    }
}

fn load_segs(
    phys: &Phys,
    bootfs: Directory,
    bootfs_phys: &Phys,
    root: &Virt,
) -> Result<elfload::LoadedElf, Error> {
    let phys = match elfload::get_interp(phys) {
        Ok(Some(mut interp)) => {
            use SError as SvError;

            let last = interp.pop();
            assert_eq!(last, Some(0), "Not a valid c string");

            let data = bootfs
                .find(&interp, b'/')
                .ok_or(SvError::ENOENT)
                .inspect_err(|_| {
                    log::error!("Failed to find the interpreter for the executable")
                })?;

            crate::sub_phys(data, bootfs, bootfs_phys)?
        }
        Ok(None) => panic!("Executables cannot be directly executed"),
        Err(err) => return Err(Error::Load(err)),
    };

    elfload::load(&phys, true, root).map_err(Error::Load)
}

pub fn load_elf(
    phys: &Phys,
    bootfs: Directory,
    bootfs_phys: &Phys,
    root: &Virt,
) -> Result<(NonNull<u8>, NonNull<u8>), Error> {
    let elf = load_segs(phys, bootfs, bootfs_phys, root)?;

    let (stack_size, stack_flags) = elf.stack.map_or(
        (
            DEFAULT_STACK_SIZE,
            Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS,
        ),
        |stack| {
            (
                if stack.0 > 0 {
                    stack.0
                } else {
                    DEFAULT_STACK_SIZE
                },
                stack.1,
            )
        },
    );

    let stack = {
        let layout = unsafe { Virt::page_aligned(stack_size) };
        let (alloc_layout, _) = layout
            .extend(STACK_PROTECTOR_LAYOUT)
            .and_then(|(layout, _)| layout.extend(STACK_PROTECTOR_LAYOUT))
            .map_err(SError::from)?;

        let virt = root.allocate(None, alloc_layout)?;
        let stack_phys = Phys::allocate(stack_size, true)?;
        let stack_ptr = virt.map(
            Some(STACK_PROTECTOR_SIZE),
            stack_phys,
            0,
            layout,
            stack_flags,
        )?;

        let base = stack_ptr.as_non_null_ptr();
        unsafe { NonNull::new_unchecked(base.as_ptr().add(stack_size)) }
    };

    Ok((
        unsafe { NonNull::new_unchecked(elf.entry as *mut u8) },
        stack,
    ))
}
