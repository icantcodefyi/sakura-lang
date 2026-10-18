//! Index-based arenas without deletion, with the API of `la-arena` 0.3 that this workspace uses,
//! plus serde: an [`Idx`] is its `u32`, an [`Arena`] its values, an [`ArenaMap`] its slots. They
//! are here because std's analysis is cached on disk (design §17.195 (3)) and `la-arena` has no
//! serde support. `Debug` output matches `la-arena`'s.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The raw index of a value in an arena.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawIdx(u32);

impl RawIdx {
    pub const fn from_u32(u32: u32) -> Self {
        RawIdx(u32)
    }

    pub const fn into_u32(self) -> u32 {
        self.0
    }
}

impl From<RawIdx> for u32 {
    fn from(raw: RawIdx) -> u32 {
        raw.0
    }
}

impl From<u32> for RawIdx {
    fn from(idx: u32) -> RawIdx {
        RawIdx(idx)
    }
}

impl fmt::Debug for RawIdx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for RawIdx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The index of a value allocated in an arena that holds `T`s.
pub struct Idx<T> {
    raw: RawIdx,
    _ty: PhantomData<fn() -> T>,
}

impl<T> Idx<T> {
    pub const fn from_raw(raw: RawIdx) -> Self {
        Idx {
            raw,
            _ty: PhantomData,
        }
    }

    pub const fn into_raw(self) -> RawIdx {
        self.raw
    }

    fn slot(self) -> usize {
        self.raw.0 as usize
    }
}

impl<T> Clone for Idx<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Idx<T> {}

impl<T> PartialEq for Idx<T> {
    fn eq(&self, other: &Idx<T>) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Idx<T> {}

impl<T> Ord for Idx<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}

impl<T> PartialOrd for Idx<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Hash for Idx<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> fmt::Debug for Idx<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut type_name = std::any::type_name::<T>();
        if let Some(idx) = type_name.rfind(':') {
            type_name = &type_name[idx + 1..];
        }
        write!(f, "Idx::<{}>({})", type_name, self.raw)
    }
}

impl<T> Serialize for Idx<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.raw.serialize(s)
    }
}

impl<'de, T> Deserialize<'de> for Idx<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        RawIdx::deserialize(d).map(Idx::from_raw)
    }
}

/// Values of one type, addressed by the [`Idx`] [`Arena::alloc`] returned.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Arena<T> {
    data: Vec<T>,
}

impl<T: fmt::Debug> fmt::Debug for Arena<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Arena")
            .field("len", &self.len())
            .field("data", &self.data)
            .finish()
    }
}

impl<T> Arena<T> {
    pub const fn new() -> Arena<T> {
        Arena { data: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn alloc(&mut self, value: T) -> Idx<T> {
        let idx = Idx::from_raw(RawIdx(self.data.len() as u32));
        self.data.push(value);
        idx
    }

    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (Idx<T>, &T)> + DoubleEndedIterator + Clone {
        (self.data.iter().enumerate()).map(|(i, v)| (Idx::from_raw(RawIdx(i as u32)), v))
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl ExactSizeIterator<Item = (Idx<T>, &mut T)> + DoubleEndedIterator {
        (self.data.iter_mut().enumerate()).map(|(i, v)| (Idx::from_raw(RawIdx(i as u32)), v))
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &T> + DoubleEndedIterator {
        self.data.iter()
    }

    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut T> + DoubleEndedIterator {
        self.data.iter_mut()
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Arena<T> {
        Arena::new()
    }
}

impl<T> Index<Idx<T>> for Arena<T> {
    type Output = T;
    fn index(&self, idx: Idx<T>) -> &T {
        &self.data[idx.slot()]
    }
}

impl<T> IndexMut<Idx<T>> for Arena<T> {
    fn index_mut(&mut self, idx: Idx<T>) -> &mut T {
        &mut self.data[idx.slot()]
    }
}

impl<T> FromIterator<T> for Arena<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Arena {
            data: Vec::from_iter(iter),
        }
    }
}

/// A map from the indexes of one arena to values; space is O(highest index).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArenaMap<IDX, V> {
    v: Vec<Option<V>>,
    _ty: PhantomData<IDX>,
}

impl<T, V> ArenaMap<Idx<T>, V> {
    pub const fn new() -> Self {
        ArenaMap {
            v: Vec::new(),
            _ty: PhantomData,
        }
    }

    pub fn contains_idx(&self, idx: Idx<T>) -> bool {
        matches!(self.v.get(idx.slot()), Some(Some(_)))
    }

    pub fn insert(&mut self, idx: Idx<T>, t: V) -> Option<V> {
        let i = idx.slot();
        self.v.resize_with((i + 1).max(self.v.len()), || None);
        self.v[i].replace(t)
    }

    pub fn remove(&mut self, idx: Idx<T>) -> Option<V> {
        self.v.get_mut(idx.slot())?.take()
    }

    pub fn get(&self, idx: Idx<T>) -> Option<&V> {
        self.v.get(idx.slot()).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, idx: Idx<T>) -> Option<&mut V> {
        self.v.get_mut(idx.slot()).and_then(Option::as_mut)
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> {
        self.v.iter().filter_map(Option::as_ref)
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (Idx<T>, &V)> {
        (self.v.iter().enumerate())
            .filter_map(|(i, o)| Some((Idx::from_raw(RawIdx(i as u32)), o.as_ref()?)))
    }
}

impl<T, V> Index<Idx<T>> for ArenaMap<Idx<T>, V> {
    type Output = V;
    fn index(&self, idx: Idx<T>) -> &V {
        self.v[idx.slot()].as_ref().unwrap()
    }
}

impl<T, V> Default for ArenaMap<Idx<T>, V> {
    fn default() -> Self {
        ArenaMap::new()
    }
}

impl<T, V> Extend<(Idx<T>, V)> for ArenaMap<Idx<T>, V> {
    fn extend<I: IntoIterator<Item = (Idx<T>, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl<T, V> FromIterator<(Idx<T>, V)> for ArenaMap<Idx<T>, V> {
    fn from_iter<I: IntoIterator<Item = (Idx<T>, V)>>(iter: I) -> Self {
        let mut map = ArenaMap::new();
        map.extend(iter);
        map
    }
}

impl<T, V: Serialize> Serialize for ArenaMap<Idx<T>, V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.v.serialize(s)
    }
}

impl<'de, T, V: Deserialize<'de>> Deserialize<'de> for ArenaMap<Idx<T>, V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(ArenaMap {
            v: Vec::deserialize(d)?,
            _ty: PhantomData,
        })
    }
}
