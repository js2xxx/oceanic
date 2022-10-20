use alloc::collections::{btree_map::Entry, BTreeMap};

use solvent::prelude::Handle;
use solvent_async::sync::Mutex;
use solvent_core::sync::Arsc;
use solvent_rpc::io::OpenOptions;

use super::DirectoryMut;

struct Conn {
    entry: Arsc<dyn DirectoryMut>,
    options: OpenOptions,
}

#[derive(Clone)]
pub struct EventTokens {
    tokens: Arsc<Mutex<BTreeMap<Handle, Conn>>>,
}

impl EventTokens {
    #[inline]
    pub fn new() -> Self {
        EventTokens {
            tokens: Arsc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that `handle` is the raw reference of a
    /// `DirectoryEventSender`.
    pub async unsafe fn insert(
        &self,
        entry: Arsc<dyn DirectoryMut>,
        handle: Handle,
        options: OpenOptions,
    ) {
        let mut tokens = self.tokens.lock_arsc().await;
        tokens.insert(handle, Conn { entry, options });
    }

    pub async fn take_if<F>(&self, handle: Handle, f: F) -> Option<Arsc<dyn DirectoryMut>>
    where
        F: FnOnce(&Arsc<dyn DirectoryMut>, OpenOptions) -> bool,
    {
        let mut tokens = self.tokens.lock_arsc().await;
        match tokens.entry(handle) {
            Entry::Occupied(ent) if f(&ent.get().entry, ent.get().options) => {
                Some(ent.remove().entry)
            }
            _ => None,
        }
    }

    pub async fn remove(&self, handle: Handle) {
        let mut tokens = self.tokens.lock_arsc().await;
        tokens.remove(&handle);
    }
}

impl Default for EventTokens {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
