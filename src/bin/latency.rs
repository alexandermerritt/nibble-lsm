
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![feature(core_intrinsics)]

// NOTE XXX
// Due to rounding errors when we insert keys in parallel during the
// setup phase, some keys may not actually exist. It is best to
// double check this happens infrequently; if it is infrequent, we can
// ignore them.

extern crate rand; // import before kvs
#[macro_use]
extern crate log;
extern crate time;
extern crate clap;
extern crate num;
extern crate crossbeam;
extern crate parking_lot as pl;

#[macro_use]
extern crate kvs;

use kvs::distributions::*;

use clap::{Arg, App, SubCommand};
use kvs::clock;
use kvs::common::{self,Pointer,ErrorCode,rdrand,rdrandq};
use kvs::logger;
use kvs::lsm::{self,LSM};
use kvs::memory;
use kvs::meta;
use kvs::numa::{self,NodeId};
use kvs::sched::*;
use kvs::segment::{ObjDesc,SEGMENT_SIZE};
use kvs::macros::*;
use log::LogLevel;
use rand::Rng;
use std::cmp;
use std::collections::VecDeque;
use std::intrinsics;
use std::mem;
use std::ptr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::*;
use std::thread::{self,JoinHandle};
use std::time::{Instant,Duration};

//==----------------------------------------------------------------==
//  Build-based functions
//  Compile against LSM, or exported functions.
//==----------------------------------------------------------------==

/// Used to create the stack-based buffers for holding GET output.
pub const MAX_KEYSIZE: usize = 1usize << 10;

#[inline(always)]
fn put_object(kvs: &mut LSM, key: u64, value: Pointer<u8>, len: usize, sock: usize) {
    let obj = ObjDesc::new(key, value, len);
    let nibnode = lsm::PutPolicy::Specific(sock);
    loop {
        let err = kvs.put_where(&obj, nibnode);
        if unlikely!(err.is_err()) {
            match err {
                Err(ErrorCode::OutOfMemory) => continue,
                _ => {
                    println!("Error: {:?}", err.unwrap());
                    unsafe { intrinsics::abort(); }
                },
            }
        } else {
            break;
        }
    }
}

#[inline(always)]
fn get_object(kvs: &mut LSM, key: u64) {
    let mut buf: [u8;MAX_KEYSIZE] =
        unsafe { mem::uninitialized() };
    let _ = kvs.get_object(key, &mut buf);
}

macro_rules! _put {
    ( $_kvs:expr, $_area:expr,
      $_k:expr, $_l:expr, $_is:expr, $_sock:expr ) => {
        let r = unsafe { rdrandq() } as usize % $_l;
        let mut v = Pointer(unsafe {$_area.offset(r as isize)});
        put_object($_kvs, $_k, v, $_is, $_sock);
    }
}

fn put_many(kvs: &mut LSM, nitems: usize, itemsz: usize, sock: usize, kbase: u64) {
    let totalsz = itemsz * nitems;
    let value = memory::allocate::<u8>(itemsz * nitems);
    let now = clock::now();
    let iter = nitems / 20;
    for k in 1..iter {
        let key = (k as u64 + kbase) * 20u64;
        _put!(kvs, value, key+0, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+1, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+2, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+3, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+4, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+5, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+6, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+7, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+8, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+9, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+10, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+11, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+12, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+13, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+14, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+15, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+16, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+17, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+18, totalsz-itemsz, itemsz, sock);
        _put!(kvs, value, key+19, totalsz-itemsz, itemsz, sock);
    }
    let el = (clock::now() - now) / nitems as u64;
    println!("cycles/op {}", el);
}

fn del_many(kvs: &mut LSM, nitems: usize, kbase: u64) {
    for key in 1..nitems {
        let _ = kvs.del_object(key as u64 +kbase);
    }
}

fn do_local() {
    let totalsz = 1usize << 37;
    let activesz = 1usize << 32;
    let itemsz = 1024;
    let nitems = activesz / itemsz;
    println!("totalsz {} activesz {} itemsz {} nitems {}",
             totalsz, activesz, itemsz, nitems);
    let mut kvs = LSM::new2(totalsz, nitems*2);
    for node in 0..numa::NODE_MAP.sockets() { kvs.enable_compaction(NodeId(node)); }

    // different keys for each socket
    let offset = 1_000_000_000_u64;
    let mut bases = vec![];
    bases.push(offset);
    bases.push(offset+nitems as u64);
    bases.push(offset+2u64*nitems as u64);
    bases.push(offset+3u64*nitems as u64);

    println!("warmup");
    for sock in 0..4 {
        let kbase = bases[sock];
        put_many(&mut kvs, nitems, itemsz, sock, kbase);
    }

    let offset = 1_u64;
    let mut bases = vec![];
    bases.push(offset);
    bases.push(offset+nitems as u64);
    bases.push(offset+2u64*nitems as u64);
    bases.push(offset+3u64*nitems as u64);

    // run
    for sock in 0..4 {
        println!("socket {}", sock);
        // insertions
        put_many(&mut kvs, nitems, itemsz, sock, bases[sock]);
        // updates
        put_many(&mut kvs, nitems, itemsz, sock, bases[sock]);
        let _ = del_many(&mut kvs, nitems, bases[sock]);
    }

}

fn do_all() {
    let totalsz = 1usize << 37;
    let activesz = 1usize << 32;
    let itemsz = 1024;
    let nitems = activesz / itemsz;
    println!("totalsz {} activesz {} itemsz {} nitems {}",
             totalsz, activesz, itemsz, nitems);
    let mut kvs = LSM::new2(totalsz, nitems*2);
    for node in 0..numa::NODE_MAP.sockets() { kvs.enable_compaction(NodeId(node)); }

    println!("warmup");
    let value = memory::allocate::<u8>(itemsz * nitems);
    let mut v = Pointer(value);
    for key in 1..nitems {
        let sock = unsafe { rdrand() } as usize % 4;
        put_object(&mut kvs, key as u64, v, itemsz, sock);
    }

    println!("running");
    let now = clock::now();
    for key in 1..nitems {
        let r = unsafe { rdrandq() } as usize % (activesz - itemsz);
        let mut v = Pointer(unsafe {value.offset(r as isize)});
        put_object(&mut kvs, key as u64, v, itemsz, 0);
    }
    let el = (clock::now() - now) as usize / nitems;
    println!("cycles/op {}", el);

}

fn main() {
    logger::enable();
    unsafe {
        pin_cpu(0);
    }
    do_local();
    //do_all();
}

