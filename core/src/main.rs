#![feature(str_from_raw_parts)]
#![feature(iter_array_chunks)]

use game::state::{Context, update_loop};

mod error;
mod game;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    update_loop().map_err(|error| error.into())
}
