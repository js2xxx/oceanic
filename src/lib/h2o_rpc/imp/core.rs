use crate as solvent_rpc;

#[protocol]
pub trait Cloneable {
    fn clone_connection(conn: SelfServer);
}

#[protocol]
pub trait Closeable {
    #[close]
    fn close_connection();
}
