#![doc = include_str!("../README.md")]
#![warn(
    unsafe_op_in_unsafe_fn,
    clippy::missing_safety_doc,
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks
)]

pub mod iter;
pub mod iter_set;
pub mod mapref;
pub mod setref;
pub mod tableref;
pub mod try_result;

mod lock;
mod map;
mod read_only;
mod set;
mod table;
mod util;

#[cfg(feature = "serde")]
mod serde;

#[cfg(feature = "rayon")]
pub mod rayon {
    pub mod map;
    pub mod read_only;
    pub mod set;
}

#[cfg(not(feature = "raw-api"))]
use crate::lock::RwLock;

#[cfg(feature = "raw-api")]
pub use crate::lock::{RawRwLock, RwLock};

use crossbeam_utils::CachePadded;
use hashbrown::hash_table;
use std::sync::OnceLock;

pub use map::ClashMap;
pub use mapref::entry::{Entry, OccupiedEntry, VacantEntry};
pub use mapref::entry_ref::{EntryRef, VacantEntryRef};
pub use read_only::ReadOnlyView;
pub use set::ClashSet;
pub use table::ClashTable;

pub(crate) type HashMap<K, V> = hash_table::HashTable<(K, V)>;
pub(crate) type Shard<K, V> = CachePadded<RwLock<HashMap<K, V>>>;

// Temporary reimplementation of [`std::collections::TryReserveError`]
// util [`std::collections::TryReserveError`] stabilises.
// We cannot easily create `std::collections` error type from `hashbrown` error type
// without access to `TryReserveError::kind` method.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TryReserveError {}

#[no_mangle]
fn default_shard_amount() -> usize {
    static DEFAULT_SHARD_AMOUNT: OnceLock<usize> = OnceLock::new();
    *DEFAULT_SHARD_AMOUNT.get_or_init(|| {
        (std::thread::available_parallelism().map_or(1, usize::from) * 4).next_power_of_two()
    })
}
