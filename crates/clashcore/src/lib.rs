//! Sharded `RwLock` primitives shared by [`clashmap`].
//!
//! `clashcore` exists so that the locking and sharding machinery underneath
//! `clashmap` can evolve on its own release cadence, independent of the
//! hashtable on top. It deliberately exposes a small, low-level API: a custom
//! [`RawRwLock`](lock::RawRwLock), the [`ClashCollection`] sharded container,
//! and the bundled-guard reference types ([`Ref`](one::Ref),
//! [`RefMut`](one::RefMut), [`RefMulti`](multiple::RefMulti),
//! [`RefMutMulti`](multiple::RefMutMulti)) that pair a held lock guard with a
//! borrow into the protected data.
//!
//! Most users should reach for [`clashmap`] instead — it builds a
//! `HashMap`-shaped concurrent API on top of the primitives here. Use
//! `clashcore` directly if you want to build a different sharded data
//! structure (e.g. a sharded `Vec` or `BTreeMap`) using the same locking
//! strategy.
//!
//! # Modules
//!
//! - [`lock`] — the custom `RawRwLock` implementation and detached guard type
//!   aliases.
//! - [`sharded`] — [`ClashCollection`], an array of cache-padded `RwLock`s
//!   addressed by hash, plus [`default_shard_amount`](sharded::default_shard_amount).
//! - [`one`] — [`Ref`](one::Ref) and [`RefMut`](one::RefMut), guard+borrow
//!   bundles for individual entries.
//! - [`multiple`] — [`RefMulti`](multiple::RefMulti) and
//!   [`RefMutMulti`](multiple::RefMutMulti), `Arc`-shared variants used by
//!   iterators that visit many entries within a single shard.
//!
//! # Stability
//!
//! `clashcore` follows semver. The detached lock guards and the
//! `Ref*::new`/`Ref*::into_parts` constructors are unsafe building blocks
//! intended for crates implementing their own sharded structures; their
//! safety contracts are documented on each item.
//!
//! [`clashmap`]: https://docs.rs/clashmap

#![warn(
    unsafe_op_in_unsafe_fn,
    clippy::missing_safety_doc,
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks
)]

pub mod lock;
pub mod multiple;
pub mod one;
pub mod sharded;

mod util;

pub use sharded::ClashCollection;

/// Re-export of [`crossbeam_utils::CachePadded`] so consumers don't need a
/// direct dependency on `crossbeam-utils` to use [`ClashCollection::shards`]
/// and friends.
pub use crossbeam_utils::CachePadded;
