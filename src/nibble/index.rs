use std::sync::Arc;

use super::super::cuckoo;
use numa;

//==----------------------------------------------------==//
//      Index
//==----------------------------------------------------==//

pub type IndexRef = Arc<Index>;

/// Fat pointer as the value in every index entry.
/// | Socket ID | Virtual Address |
///     16 bits     48 bits
pub type IndexEntry = u64;

/// Decompose an IndexEntry
#[inline(always)]
pub fn extract(entry: IndexEntry) -> (u16,u64) {
    ( (entry >> 48) as u16, entry & ((1u64<<48)-1) )
}

/// Create an index entry from the Socket ID and virtual address
#[inline(always)]
pub fn merge(socket: u16, va: u64) -> IndexEntry {
    ((socket as u64) << 48) | (va & ((1u64<<48)-1))
}

/// Index structure that allows us to retreive objects from the log.
/// It is just a simple wrapper over whatever data structure we wish
/// to eventually use.
/// TODO what if i store the hash of a key instead of the key itself?
/// save space? all the keys are kept in the log anyway
pub struct Index {
    // cuckoo interface has no Rust state (behind FFI)
}

impl Index {

    // FIXME pass some Config object that says how the index should be
    // allocated across NUMA sockets
    pub fn new() -> Self {
        let nnodes = numa::NODE_MAP.sockets();
        let mask: usize = (1usize<<nnodes)-1;
        cuckoo::init(mask, nnodes); // FIXME should only be called once
        Index { }
    }

    /// Return value of object if it exists, else None.
    #[inline(always)]
    pub fn get(&self, key: u64) -> Option<IndexEntry> {
        cuckoo::find(key)
    }

    /// Update location of object in the index. Returns None if object
    /// was newly inserted, or the virtual address of the prior
    /// object.
    #[inline(always)]
    pub fn update(&self, key: u64, value: IndexEntry)
        -> Option<IndexEntry> {
        cuckoo::update(key, value)
    }

    /// Remove an entry. If it existed, return value, else return
    /// None.
    #[inline(always)]
    pub fn remove(&self, key: u64) -> Option<IndexEntry> {
        let mut val: IndexEntry = 0;
        match cuckoo::erase(key, &mut val) {
            true => Some(val),
            false => None,
        }
    }

    pub fn len(&self) -> usize {
        cuckoo::size()
    }
}

//==----------------------------------------------------==//
//      Unit tests
//==----------------------------------------------------==//

#[cfg(IGNORE)]
mod tests {
    use super::*;

    use super::super::logger;

    #[test]
    fn index_basic() {
        logger::enable();
        let index = Index::new();
        let key1 = String::from("alex");
        let key2 = String::from("notexist");
        assert_eq!(index.update(&key1, 42).is_some(), false);
        let opt = index.update(&key1, 24);
        assert_eq!(opt.is_some(), true);
        assert_eq!(opt.unwrap(), 42);
        assert_eq!(index.get(key2.as_str()).is_some(), false);
        assert_eq!(index.get(key1.as_str()).is_some(), true);
    }
}
