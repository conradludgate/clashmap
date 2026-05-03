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
