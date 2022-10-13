pub trait Directory {}

pub trait DirIter {}

pub mod sync {
    use solvent::prelude::Channel;
    use solvent_rpc::io::{
        dir::{dir_iter_sync, directory_sync, DirEntry},
        Error,
    };

    #[derive(Clone)]
    pub struct Remote(pub directory_sync::DirectoryClient);

    impl Remote {
        #[inline]
        pub fn iter(&self) -> Result<RemoteIter, Error> {
            let (t, conn) = Channel::new();
            let iter = dir_iter_sync::DirIterClient::from(t);
            self.0.iter(conn)??;
            Ok(RemoteIter(iter))
        }
    }

    #[derive(Clone)]
    pub struct RemoteIter(pub dir_iter_sync::DirIterClient);

    impl Iterator for RemoteIter {
        type Item = Result<DirEntry, Error>;

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            self.0.next().ok()
        }
    }
}
