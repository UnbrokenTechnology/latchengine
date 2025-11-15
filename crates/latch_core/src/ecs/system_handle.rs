use std::fmt;

/// Handle assigned to each registered system.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SystemHandle(u32);

impl SystemHandle {
    pub(crate) fn new(index: u32) -> Self {
        Self(index)
    }

    /// Return the raw index backing this handle.
    #[inline]
    pub fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SystemHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
