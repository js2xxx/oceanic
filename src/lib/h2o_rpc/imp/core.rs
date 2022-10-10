use crate as solvent_rpc;

#[protocol]
pub trait Cloneable {
    fn clone_client() -> Self;
}
