use std::ops::Index;

/// A fixed size array.
#[derive(Debug)]
pub struct Array<T, const N: usize> {
    inner: [Option<T>; N],
    len: usize,
}

impl<T, const N: usize> Array<T, N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: [const { None }; N],
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

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, T, N> {
        Iter {
            array: self,
            index: 0,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T, N> {
        IterMut {
            array: self,
            index: 0,
        }
    }
}

impl<T: Clone, const N: usize> Clone for Array<T, N> {
    fn clone(&self) -> Self {
        let mut array = Array::<T, N>::new();
        for item in self {
            array.push(item.clone());
        }
        array
    }
}

impl<T: Copy, const N: usize> Copy for Array<T, N> {}

impl<T: PartialEq, const N: usize> PartialEq for Array<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T, const N: usize> Index<usize> for Array<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.inner[index].as_ref().unwrap()
    }
}

impl<T: Eq, const N: usize> Eq for Array<T, N> {}

impl<T, const N: usize> Default for Array<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, const N: usize> FromIterator<A> for Array<A, N> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut array = Array::new();
        for elem in iter {
            array.push(elem);
        }
        array
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a Array<T, N> {
    type Item = &'a T;

    type IntoIter = Iter<'a, T, N>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, const N: usize> IntoIterator for Array<T, N> {
    type Item = T;

    type IntoIter = IntoIter<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            array: self,
            index: 0,
        }
    }
}

pub struct Iter<'a, T: 'a, const N: usize> {
    array: &'a Array<T, N>,
    index: usize,
}

impl<'a, T, const N: usize> Iterator for Iter<'a, T, N> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let index = advance_iter_index(&mut self.index, self.array.len)?;
        self.array.inner[index].as_ref()
    }
}

pub struct IterMut<'a, T: 'a, const N: usize> {
    array: &'a mut Array<T, N>,
    index: usize,
}

impl<'a, T, const N: usize> Iterator for IterMut<'a, T, N> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        let index = advance_iter_index(&mut self.index, self.array.len)?;
        unsafe { &mut *self.array.inner.as_mut_ptr().add(index) }.as_mut()
    }
}

pub struct IntoIter<T, const N: usize> {
    array: Array<T, N>,
    index: usize,
}

impl<T, const N: usize> Iterator for IntoIter<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let index = advance_iter_index(&mut self.index, self.array.len)?;
        self.array.inner[index].take()
    }
}

#[inline]
fn advance_iter_index(index: &mut usize, len: usize) -> Option<usize> {
    if *index >= len {
        return None;
    }
    let old_index = *index;
    *index += 1;
    Some(old_index)
}

#[cfg(test)]
mod tests {
    use super::Array;

    #[test]
    fn push() {
        let mut array = Array::<u32, 1002>::new();
        let mut vec = Vec::new();
        for i in 0..1000 {
            array.push(i);
            vec.push(i);
        }
        assert_eq!(array.len(), 1000);
        assert_eq!(array.into_iter().collect::<Vec<_>>(), vec);
    }

    #[test]
    fn into_iter() {
        let mut vec = Vec::new();
        for i in 0..1000 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1002>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.into_iter().zip(0..1000) {
            assert_eq!(elem, i);
        }

        let mut vec = Vec::new();
        for i in 333..555 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1002>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.into_iter().zip(333..555) {
            assert_eq!(elem, i);
        }
    }

    #[test]
    fn iter() {
        let mut vec = Vec::new();
        for i in 0..1000 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1002>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.iter().zip(0..1000) {
            assert_eq!(elem, &i);
        }

        let mut vec = Vec::new();
        for i in 333..555 {
            vec.push(i);
        }
        let len = vec.len();
        let array = Array::<u32, 1002>::from_iter(vec);

        assert_eq!(len, array.len());
        for (elem, i) in array.iter().zip(333..555) {
            assert_eq!(elem, &i);
        }
    }
}
