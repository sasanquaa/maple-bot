use std::{
    mem,
    ops::{Deref, Index},
};

/// A Copy fixed size array.
#[derive(Debug, Clone, Copy)]
pub struct Array<T: Copy, const N: usize> {
    inner: [Option<T>; N],
    len: usize,
}

impl<T: Copy + PartialEq, const N: usize> PartialEq for Array<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Copy, const N: usize> Deref for Array<T, N> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: `Option<T>` can be safely transmuted to `T` as part of Rust guaranteed
        unsafe { mem::transmute::<&[Option<T>], &[T]>(&self.inner[0..self.len]) }
    }
}

impl<T: Copy, const N: usize> Index<usize> for Array<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.inner[index].as_ref().unwrap()
    }
}

impl<T: Copy + Eq, const N: usize> Eq for Array<T, N> {}

impl<T: Copy, const N: usize> Default for Array<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const N: usize> Array<T, N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: [None; N],
            len: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, value: T) {
        assert!(self.len < N);
        let index = self.len;
        self.len += 1;
        self.inner[index] = Some(value);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    // TODO: ???
    #[inline]
    pub fn consume<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        *self = Array::from_iter(iter);
    }
}

impl<A: Copy, const N: usize> FromIterator<A> for Array<A, N> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut array = Array::new();
        for elem in iter {
            array.push(elem);
        }
        array
    }
}

impl<T: Copy, const N: usize> IntoIterator for Array<T, N> {
    type Item = T;

    type IntoIter = ArrayIterator<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        ArrayIterator {
            array: self,
            index: 0,
        }
    }
}

pub struct ArrayIterator<T: Copy, const N: usize> {
    array: Array<T, N>,
    index: usize,
}

impl<T: Copy, const N: usize> Iterator for ArrayIterator<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.array.len {
            return None;
        }
        let index = self.index;
        self.index += 1;
        self.array.inner[index]
    }
}

#[cfg(test)]
mod tests {
    use super::Array;

    #[test]
    fn push() {
        let mut array = Array::<u32, 1000>::new();
        let mut vec = Vec::new();
        for i in 0..1000 {
            array.push(i);
            vec.push(Some(i));
        }
        assert_eq!(array.len, 1000);
        assert_eq!(&array.inner, vec.as_slice());
    }

    // #[test]
    // fn remove() {
    //     let mut array = Array::<u32, 7>::new();
    //     for i in 0..7 {
    //         array.push(i);
    //     }

    //     array.remove(1);
    //     assert_eq!(array.len, 6);
    //     assert_eq!(
    //         [Some(0), Some(2), Some(3), Some(4), Some(5), Some(6), None],
    //         array.inner
    //     );

    //     array.remove(2);
    //     assert_eq!(array.len, 5);
    //     assert_eq!(
    //         [Some(0), Some(2), Some(4), Some(5), Some(6), None, None],
    //         array.inner
    //     );

    //     array.remove(3);
    //     assert_eq!(array.len, 4);
    //     assert_eq!(
    //         [Some(0), Some(2), Some(4), Some(6), None, None, None],
    //         array.inner
    //     );

    //     array.remove(0);
    //     assert_eq!(array.len, 3);
    //     assert_eq!(
    //         [Some(2), Some(4), Some(6), None, None, None, None],
    //         array.inner
    //     );
    // }

    #[test]
    fn iter() {
        let mut array = Array::<u32, 1000>::new();
        let mut vec = Vec::new();
        for i in 0..1000 {
            vec.push(i);
        }
        let len = vec.len();
        array.consume(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.into_iter().zip(0..1000) {
            assert_eq!(elem, i);
        }

        let mut vec = Vec::new();
        for i in 333..555 {
            vec.push(i);
        }
        let len = vec.len();
        array.consume(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.into_iter().zip(333..555) {
            assert_eq!(elem, i);
        }
    }
}
