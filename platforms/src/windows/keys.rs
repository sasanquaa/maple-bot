use super::handle::Handle;

#[derive(Clone, Debug)]
pub struct Keys {
    handle: Handle,
}

impl Keys {
    pub fn new(handle: Handle) -> Self {
        Self { handle }
    }
}
