pub struct Combine<T1, T2> {
    pos: bool,
    t1: T1,
    t2: T2,
}

impl<T1, T2> Iterator for Combine<T1, T2>
where
    T1: Iterator,
    T2: Iterator<Item = T1::Item>,
{
    type Item = T1::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos {
            self.pos = false;
            self.t2.next()
        } else {
            self.pos = true;
            self.t1.next()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let r1 = self.t1.size_hint();
        let r2 = self.t2.size_hint();
        (r1.0 + r2.0, r1.1.and_then(|x| r2.1.map(|y| x + y)))
    }
}

pub trait CombineIter: Iterator {
    fn combine<T>(self, other: T) -> Combine<Self, T::IntoIter>
    where
        Self: Sized,
        T: IntoIterator<Item = Self::Item>,
    {
        Combine {
            pos: false,
            t1: self,
            t2: other.into_iter(),
        }
    }
}

impl<T> CombineIter for T where T: Iterator {}
