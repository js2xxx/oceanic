mod ipc;
mod mem;
mod task;

pub unsafe fn test_syscall() {
    let stack = task::test();
    ipc::test(stack);
    mem::test();
}
