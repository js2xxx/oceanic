pub const DEFAULT_STACK_SIZE: usize = 256 * 1024;

pub fn exit<T>(res: crate::Result<T>) -> !
where
    T: crate::SerdeReg,
{
    let retval = crate::Error::encode(res.map(|o| o.encode()));
    let _ = crate::call::task_exit(retval);
    unreachable!();
}
