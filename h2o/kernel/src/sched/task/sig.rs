#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signal {
    Kill,
    Suspend,
}

impl TryFrom<u32> for Signal {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Signal::Kill),
            2 => Ok(Signal::Suspend),
            _ => Err(value),
        }
    }
}
