use solvent::prelude::Virt;

mod ipc;
mod mem;
mod task;

pub unsafe fn test_syscall(virt: &Virt) {
    let stack = task::test(virt);
    ipc::test(virt, stack);
    mem::test(virt);
}
