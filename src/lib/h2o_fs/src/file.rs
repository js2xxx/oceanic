use solvent_async::io::Stream;

use crate::entry::Entry;

pub trait MmappedFile: Entry {
    fn stream() -> Stream;

    fn flush();
}
