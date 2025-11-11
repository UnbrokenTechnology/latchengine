pub struct Page<T: Default> {
    storage: Vec<T>,
    free: Vec<usize>,
}

impl<T: Default> Page<T> {

    const DEFAULT_CAPACITY: usize = 64;

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(n: usize) -> Self {
        let storage = vec![T::default(); n];
        let mut free = Vec::with_capacity(n);
        for i in (0..n).rev() { free.push(i); } // LIFO
        Self { storage, free }
    }

    pub fn get(&mut self) -> Element<T> {
        let idx = self.free.pop().expect("Block is full");
        Element {
            block: Mutex::new(self),
            idx,
            ptr: &mut self.storage[idx],
        }
    }

    pub fn free(&mut self, idx: usize) {
        self.free.push(idx);
    }

    pub fn empty(&self) -> bool {
        self.free.is_empty()
    }

}