struct Element<'a, T> {
    block: &'a Arc<Mutex<Block<T>>>,
    idx: usize,
    ptr: &mut T,
}

impl<'a, T> Deref for Element<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { self.ptr }
}

impl<'a, T> DerefMut for Element<'a, T> {
    fn deref_mut(&mut self) -> &mut T { self.ptr }
}

impl<'a, T> Drop for Element<'a, T> {
    fn drop(&mut self) {
        let block = self.block.lock().unwrap();
        block.free(self.idx);
    }
}
