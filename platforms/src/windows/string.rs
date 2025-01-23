use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

trait IntoPush<T> {
    fn into_push(self, item: T) -> Self;
}

impl<T> IntoPush<T> for Vec<T> {
    fn into_push(mut self, item: T) -> Self {
        self.push(item);
        self
    }
}

pub fn to_u8s<S: AsRef<OsStr>>(s: S) -> Option<Vec<u8>> {
    fn inner(s: &OsStr) -> Option<Vec<u8>> {
        s.to_str().and_then(|s| {
            let vec = s.bytes().collect::<Vec<u8>>();
            if vec.iter().any(|&u| u == 0) {
                return None;
            }
            Some(vec.into_push(0))
        })
    }
    inner(s.as_ref())
}

pub fn to_u16s<S: AsRef<OsStr>>(s: S) -> Option<Vec<u16>> {
    fn inner(s: &OsStr) -> Option<Vec<u16>> {
        let vec = s.encode_wide().collect::<Vec<u16>>();
        if vec.iter().any(|&u| u == 0) {
            return None;
        }
        Some(vec.into_push(0))
    }
    inner(s.as_ref())
}
