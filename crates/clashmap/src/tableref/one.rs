pub use clashcore::one::{Ref, RefMut};

/// Alias for [`Ref`] kept for backwards compatibility with `clashmap` 1.x,
/// where mapped and direct refs were distinct types.
pub type MappedRef<'a, T> = Ref<'a, T>;

/// Alias for [`RefMut`] kept for backwards compatibility with `clashmap` 1.x,
/// where mapped and direct refs were distinct types.
pub type MappedRefMut<'a, T> = RefMut<'a, T>;

#[cfg(test)]
mod tests {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};

    use crate::ClashTable;

    fn hash_one(s: &impl BuildHasher, h: impl Hash) -> u64 {
        let mut s = s.build_hasher();
        h.hash(&mut s);
        s.finish()
    }

    #[test]
    fn downgrade() {
        let data = ClashTable::new();
        let hasher = RandomState::new();

        data.entry(
            hash_one(&hasher, "test"),
            |&t| t == "test",
            |t| hash_one(&hasher, t),
        )
        .or_insert("test");

        let mut w_ref = data
            .find_mut(hash_one(&hasher, "test"), |&t| t == "test")
            .unwrap();

        *w_ref.value_mut() = "test2";
        let r_ref = w_ref.downgrade();
        assert_eq!(*r_ref.value(), "test2");
    }

    #[test]
    fn mapped_mut() {
        let data = ClashTable::new();
        let hasher = RandomState::new();

        data.entry(
            hash_one(&hasher, *b"test"),
            |&t| t == *b"test",
            |t| hash_one(&hasher, t),
        )
        .or_insert(*b"test");

        let b_ref = data
            .find_mut(hash_one(&hasher, *b"test"), |&t| t == *b"test")
            .unwrap();

        let s_ref = b_ref.try_map(|b| std::str::from_utf8_mut(b).ok()).unwrap();
        let mut t_ref = s_ref.try_map(|s| s.get_mut(1..)).unwrap();
        t_ref.value_mut().make_ascii_uppercase();
    }
}
