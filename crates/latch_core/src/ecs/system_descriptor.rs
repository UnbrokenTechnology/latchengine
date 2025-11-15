use crate::ecs::ComponentId;

/// Metadata describing how a system interacts with the ECS world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemDescriptor {
    name: String,
    reads: Vec<ComponentId>,
    writes: Vec<ComponentId>,
    components: Vec<ComponentId>,
}

impl SystemDescriptor {
    /// Create a new descriptor with the provided name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
            components: Vec::new(),
        }
    }

    /// Replace the read-only component set for this system.
    pub fn reads<I>(mut self, components: I) -> Self
    where
        I: IntoIterator<Item = ComponentId>,
    {
        self.reads = Self::sanitize(components);
        self.rebuild_components();
        self
    }

    /// Replace the write component set for this system.
    pub fn writes<I>(mut self, components: I) -> Self
    where
        I: IntoIterator<Item = ComponentId>,
    {
        self.writes = Self::sanitize(components);
        self.rebuild_components();
        self
    }

    /// Append a single read component.
    pub fn add_read(&mut self, component: ComponentId) {
        self.reads.push(component);
        self.rebuild_components();
    }

    /// Append a single write component.
    pub fn add_write(&mut self, component: ComponentId) {
        self.writes.push(component);
        self.rebuild_components();
    }

    /// Unique system name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Read-only component set.
    pub fn read_components(&self) -> &[ComponentId] {
        &self.reads
    }

    /// Writable component set.
    pub fn write_components(&self) -> &[ComponentId] {
        &self.writes
    }

    /// Union of read and write component ids.
    pub fn all_components(&self) -> &[ComponentId] {
        &self.components
    }

    /// Whether the descriptor touches any components at all.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    fn rebuild_components(&mut self) {
        self.reads = Self::sanitize(std::mem::take(&mut self.reads));
        self.writes = Self::sanitize(std::mem::take(&mut self.writes));
        self.components.clear();
        self.components.extend(&self.reads);
        self.components.extend(&self.writes);
        self.components.sort_unstable();
        self.components.dedup();
    }

    fn sanitize<I>(components: I) -> Vec<ComponentId>
    where
        I: IntoIterator<Item = ComponentId>,
    {
        let mut list: Vec<ComponentId> = components.into_iter().collect();
        list.sort_unstable();
        list.dedup();
        list
    }
}
