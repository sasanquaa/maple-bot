use anyhow::{Error, Ok, bail};
use bit_vec::BitVec;
use input::key_input_client::KeyInputClient;
use input::{Key, KeyRequest};
use platforms::windows::KeyKind;
use tokio::runtime::Handle;
use tokio::task::block_in_place;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};

mod input {
    tonic::include_proto!("input");
}

#[derive(Debug)]
pub struct KeysService {
    client: KeyInputClient<Channel>,
    key_down: BitVec, // TODO: is a bit wrong good?
}

impl KeysService {
    pub fn connect<D>(dest: D) -> Result<Self, Error>
    where
        D: TryInto<Endpoint>,
        D::Error: std::error::Error + Send + Sync + 'static,
    {
        let client = block_future(async move { KeyInputClient::connect(dest).await })?;
        Ok(Self {
            client,
            key_down: BitVec::from_elem(128, false),
        })
    }

    pub fn reset(&mut self) {
        for i in 0..self.key_down.len() {
            if Key::try_from(i as i32).is_ok() {
                let _ = block_future(async {
                    self.client
                        .send_up(Request::new(KeyRequest { key: i as i32 }))
                        .await
                });
            }
        }
        self.key_down.clear();
    }

    pub fn send(&mut self, key: KeyKind) -> Result<(), Error> {
        Ok(block_future(async move {
            self.client.send(to_request(key)).await?;
            self.key_down
                .set(i32::from(from_key_kind(key)) as usize, false);
            Ok(())
        })?)
    }

    pub fn send_up(&mut self, key: KeyKind) -> Result<(), Error> {
        if !self.can_send_key(key, false) {
            bail!("key not sent");
        }
        Ok(block_future(async move {
            self.client.send_up(to_request(key)).await?;
            self.key_down
                .set(i32::from(from_key_kind(key)) as usize, false);
            Ok(())
        })?)
    }

    pub fn send_down(&mut self, key: KeyKind) -> Result<(), Error> {
        if !self.can_send_key(key, true) {
            bail!("key not sent");
        }
        Ok(block_future(async move {
            self.client.send_down(to_request(key)).await?;
            self.key_down
                .set(i32::from(from_key_kind(key)) as usize, true);
            Ok(())
        })?)
    }

    #[inline]
    fn can_send_key(&self, key: KeyKind, is_down: bool) -> bool {
        let key = from_key_kind(key);
        let key_num = i32::from(key) as usize;
        let was_down = self.key_down.get(key_num).unwrap();
        !matches!((was_down, is_down), (true, true) | (false, false))
    }
}

#[inline]
fn block_future<F: Future>(f: F) -> F::Output {
    block_in_place(|| Handle::current().block_on(f))
}

#[inline]
fn to_request(key: KeyKind) -> Request<KeyRequest> {
    Request::new(KeyRequest {
        key: from_key_kind(key).into(),
    })
}

#[inline]
fn from_key_kind(key: KeyKind) -> Key {
    match key {
        KeyKind::A => Key::A,
        KeyKind::B => Key::B,
        KeyKind::C => Key::C,
        KeyKind::D => Key::D,
        KeyKind::E => Key::E,
        KeyKind::F => Key::F,
        KeyKind::G => Key::G,
        KeyKind::H => Key::H,
        KeyKind::I => Key::I,
        KeyKind::J => Key::J,
        KeyKind::K => Key::K,
        KeyKind::L => Key::L,
        KeyKind::M => Key::M,
        KeyKind::N => Key::N,
        KeyKind::O => Key::O,
        KeyKind::P => Key::P,
        KeyKind::Q => Key::Q,
        KeyKind::R => Key::R,
        KeyKind::S => Key::S,
        KeyKind::T => Key::T,
        KeyKind::U => Key::U,
        KeyKind::V => Key::V,
        KeyKind::W => Key::W,
        KeyKind::X => Key::X,
        KeyKind::Y => Key::Y,
        KeyKind::Z => Key::Z,
        KeyKind::Zero => Key::Zero,
        KeyKind::One => Key::One,
        KeyKind::Two => Key::Two,
        KeyKind::Three => Key::Three,
        KeyKind::Four => Key::Four,
        KeyKind::Five => Key::Five,
        KeyKind::Six => Key::Six,
        KeyKind::Seven => Key::Seven,
        KeyKind::Eight => Key::Eight,
        KeyKind::Nine => Key::Nine,
        KeyKind::F1 => Key::F1,
        KeyKind::F2 => Key::F2,
        KeyKind::F3 => Key::F3,
        KeyKind::F4 => Key::F4,
        KeyKind::F5 => Key::F5,
        KeyKind::F6 => Key::F6,
        KeyKind::F7 => Key::F7,
        KeyKind::F8 => Key::F8,
        KeyKind::F9 => Key::F9,
        KeyKind::F10 => Key::F10,
        KeyKind::F11 => Key::F11,
        KeyKind::F12 => Key::F12,
        KeyKind::Up => Key::Up,
        KeyKind::Down => Key::Down,
        KeyKind::Left => Key::Left,
        KeyKind::Right => Key::Right,
        KeyKind::Home => Key::Home,
        KeyKind::End => Key::End,
        KeyKind::PageUp => Key::PageUp,
        KeyKind::PageDown => Key::PageDown,
        KeyKind::Insert => Key::Insert,
        KeyKind::Delete => Key::Delete,
        KeyKind::Ctrl => Key::Ctrl,
        KeyKind::Enter => Key::Enter,
        KeyKind::Space => Key::Space,
        KeyKind::Tilde => Key::Tilde,
        KeyKind::Quote => Key::Quote,
        KeyKind::Semicolon => Key::Semicolon,
        KeyKind::Comma => Key::Comma,
        KeyKind::Period => Key::Period,
        KeyKind::Slash => Key::Slash,
        KeyKind::Esc => Key::Esc,
        KeyKind::Shift => Key::Shift,
        KeyKind::Alt => Key::Alt,
    }
}

#[cfg(test)]
mod test {
    // TODO HOW TO?
}
