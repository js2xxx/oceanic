pub fn exit<T>(res: crate::Result<T>) -> !
where
      T: crate::SerdeReg,
{
      let retval = crate::Error::encode(res.map(|o| o.encode()));
      let _ = crate::call::exit(retval);
      unreachable!();
}
