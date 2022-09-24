

/// # Arguments
/// 
/// `macro` - a macro such as `example!(type)`
#[macro_export]
macro_rules! impl_obj_for {
    ($macro:ident) => {
        $macro! ($crate::ipc::Channel);
        $macro! ($crate::task::Task);
        $macro! ($crate::task::SuspendToken);
        $macro! ($crate::mem::Space);
        $macro! ($crate::mem::Virt);
        $macro! ($crate::mem::Phys);
        $macro! ($crate::dev::Interrupt);
        $macro! ($crate::dev::MemRes);
        $macro! ($crate::dev::GsiRes);
        $macro! ($crate::dev::PioRes);
        $macro! ($crate::time::Timer);
        $macro! ($crate::obj::Dispatcher);
    };
}