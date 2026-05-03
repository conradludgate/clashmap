use super::one::RefMut;
use crate::{tableref, OccupiedEntry};

/// A view into a single entry in a map keyed by a borrowed form of the key.
///
/// Constructed via [`ClashMap::entry_ref`](crate::ClashMap::entry_ref). The
/// owned key is only materialised on insert (via [`ToOwned`]), so callers can
/// look up by `&str` against `ClashMap<String, _>`, `&Path` against
/// `ClashMap<PathBuf, _>`, etc., without paying the allocation when the entry
/// is already occupied.
pub enum EntryRef<'a, 'b, K, Q: ?Sized, V> {
    Occupied(OccupiedEntry<'a, K, V>),
    Vacant(VacantEntryRef<'a, 'b, K, Q, V>),
}

/// A view into a vacant entry obtained from [`EntryRef`].
///
/// Holds the borrowed lookup key and only materialises an owned `K` (via
/// [`ToOwned`]) when [`insert`](Self::insert) is called.
pub struct VacantEntryRef<'a, 'b, K, Q: ?Sized, V> {
    entry: tableref::entry::VacantEntry<'a, (K, V)>,
    key: &'b Q,
}

impl<'a, 'b, K, Q: ?Sized, V> VacantEntryRef<'a, 'b, K, Q, V> {
    pub(crate) fn new(entry: tableref::entry::VacantEntry<'a, (K, V)>, key: &'b Q) -> Self {
        Self { entry, key }
    }

    pub fn key(&self) -> &Q {
        self.key
    }

    pub fn insert(self, value: V) -> RefMut<'a, K, V>
    where
        Q: ToOwned<Owned = K>,
    {
        self.entry.insert((self.key.to_owned(), value)).into()
    }
}
