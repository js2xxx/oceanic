mod task;
mod ipc;
mod mem;

pub fn test_syscall() {
    // #[cfg(debug_assertions)]
    {
        let stack = task::test();
        ipc::test(stack);
        mem::test();
    }
}
