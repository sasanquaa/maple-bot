#![feature(str_from_raw_parts)]
#![feature(iter_array_chunks)]
#![feature(slice_pattern)]

use game::state::update_loop;

mod game;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    update_loop().map_err(|error| error.into())
}
