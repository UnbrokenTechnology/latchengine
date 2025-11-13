//! Entity identifiers and lookup helpers.
//!
//! Entities are represented as 64-bit handles that encode the dense
//! index alongside a generation counter. World storage keeps a map
//! from `Entity` to `EntityLoc`, allowing quick validation and lookup
//! of archetype/row information without embedding location data in
use crate::ecs::ArchetypeId;

/// Dense index type used inside packed archetype storage.
pub type EntityId = u32;
/// Generation counter used to guard against stale handles.
pub type Generation = u32;
/// External entity handle (opaque to users).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entity(u64);

impl Entity {
	const INDEX_BITS: u64 = 32;
	const INDEX_MASK: u64 = (1u64 << Self::INDEX_BITS) - 1;

	#[inline]
	pub fn new(index: EntityId, generation: Generation) -> Self {
	let index_part = index as u64 & Self::INDEX_MASK;
	let gen_part = (generation as u64) << Self::INDEX_BITS;
		Self(gen_part | index_part)
	}
	#[inline]
	pub fn index(self) -> EntityId {
		(self.0 & Self::INDEX_MASK) as EntityId
	}
	#[inline]
	pub fn generation(self) -> Generation {
		(self.0 >> Self::INDEX_BITS) as Generation
	}
	#[inline]
	pub fn to_bits(self) -> u64 {
		self.0
	}
	#[inline]
	pub fn from_bits(bits: u64) -> Self {
		Self(bits)
	}
}

/// Location of a live entity inside world storage.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EntityLoc {
	pub archetype: ArchetypeId,
	pub index: usize,
	pub generation: Generation,
}

impl EntityLoc {
	#[inline]
	pub fn new(archetype: ArchetypeId, index: usize, generation: Generation) -> Self {
		Self { archetype, index, generation }
	}
}
