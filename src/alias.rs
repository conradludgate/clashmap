use std::marker::PhantomData;

pub trait Aliasor<T> {
    fn alias<const N: usize>(src: *const T, dst: *mut T);
}

pub struct DoCopy<T> {
    _marker: PhantomData<T>,
}

impl<T> Aliasor<T> for DoCopy<T> where T:Copy {
    fn alias<const N: usize>(src: *const T, dst: *mut T) {
        unsafe {
            std::ptr::copy_nonoverlapping(src, dst, N);
        }
    }
}

pub struct DoClone<T> {
    _marker: PhantomData<T>,
}

impl<T> Aliasor<T> for DoClone<T> where T:Clone {
    fn alias<const N: usize>(src: *const T, dst: *mut T) {
        unsafe {
            for i in 0..N {
                let v = (*src.add(i)).clone();
                dst.add(i).write(v);
            }
        }
    }
}
