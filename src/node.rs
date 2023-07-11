use std::mem::MaybeUninit;
use std::mem::ManuallyDrop;

const BRANCHING_FACTOR: usize = 4;

// how many entries we can index with log2_n bits
const fn node_cap(log2_n: usize) -> usize {
    1 << log2_n
}

// 2 bits per entry and 8 bits per byte
const fn metadata_len(log2_n: usize) -> usize {
    node_cap(log2_n) * 2 / 8
}

#[repr(u8)]
enum Kind {
    Empty = 0b00,
    Leaf = 0b01,
    Branch = 0b10,
}

union Slot<T, N> {
    leaf: ManuallyDrop<MaybeUninit<T>>,
    branch: *mut N,
}

pub struct Node<const METADATA_BITS: usize, const CAPACITY: usize, T> {
    metadata: [u8; METADATA_BITS],
    slots: [Slot<T, Self>; CAPACITY],
}
