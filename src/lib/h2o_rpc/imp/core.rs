use crate as solvent_rpc;

use solvent::ipc::Channel;

#[protocol]
pub trait Cloneable {
    fn clone_connection(conn: Channel);
}

#[protocol]
pub trait Closeable {
    #[close]
    fn close_connection();
}
