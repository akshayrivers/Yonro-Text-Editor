use crate::buffers::Buffer;
use std::collections::HashMap;

#[derive(Default)]
pub struct BufferManager {
    buffers: HashMap<usize, Buffer>,
    next_buffer_id: usize,
}

impl BufferManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, buffer: Buffer) -> usize {
        let id = self.next_buffer_id;
        self.buffers.insert(id, buffer);
        self.next_buffer_id += 1;
        id
    }

    pub fn get(&self, id: usize) -> Option<&Buffer> {
        self.buffers.get(&id)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Buffer> {
        self.buffers.get_mut(&id)
    }

    pub fn remove(&mut self, id: usize) -> Option<Buffer> {
        self.buffers.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&usize, &Buffer)> {
        self.buffers.iter()
    }
}
