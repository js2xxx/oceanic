use spin::Mutex;

use super::WaitObject;

#[derive(Debug)]
pub struct WaitCell<T> {
    data: Mutex<Option<T>>,
    wo: WaitObject,
}

impl<T> WaitCell<T> {
    pub fn new() -> Self {
        WaitCell {
            data: Mutex::new(None),
            wo: WaitObject::new(),
        }
    }

    pub fn take(&self, block_desc: &'static str) -> T {
        loop {
            let mut data = self.data.lock();
            if let Some(obj) = data.take() {
                break obj;
            }
            self.wo.wait(data, block_desc);
        }
    }

    pub fn try_take(&self) -> Option<T> {
        self.data.lock().take()
    }

    pub fn replace(&self, obj: T) -> Option<T> {
        let old = self.data.lock().replace(obj);
        self.wo.notify(None);
        old
    }
}
