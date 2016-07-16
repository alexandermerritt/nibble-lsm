#![feature(test)]
#![feature(const_fn)]
#![feature(asm)]
#![feature(repr_simd)]

// Clippy tool
//#![feature(plugin)]
//#![plugin(clippy)]

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

// Remove these XXX
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unreachable_code)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate lazy_static;

extern crate libc;
extern crate rand;
extern crate test;
extern crate time;
extern crate crossbeam;
extern crate itertools;
extern crate quicksort;
extern crate syscall;
extern crate parking_lot;

// TODO keep cuckoo private and move the unit test in the integration
// test code to where it should be
pub mod cuckoo;

pub mod nibble;
pub use nibble::*;
