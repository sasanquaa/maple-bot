#![feature(str_from_raw_parts)]
#![feature(iter_array_chunks)]
#![feature(slice_pattern)]
#![feature(variant_count)]
#![feature(let_chains)]
#![feature(associated_type_defaults)]

pub mod context;
#[cfg(debug_assertions)]
mod debug;
mod detect;
mod mat;
pub mod minimap;
mod models;
pub mod player;
mod rotator;
pub mod skill;
