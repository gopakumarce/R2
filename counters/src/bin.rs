use common::pow2_u32;
use std::cmp;
use std::collections::VecDeque;

/// The Bin structure divides a range of memory into bins, each bin holds multiple objects of
/// the same size. And different bins have different object sizes, all powers of two. Once an
/// object is put in a bin, it cant be 'moved' to any other bin. The memory range is assumed
/// to be from 0 to a max size, and the objects are returned with 0 based offsets in that range,
/// the caller can adjust the offsets to real memory addresses
#[derive(Default)]
pub struct Bin {
    binsz: u32,
    max: u64,
    zeroes: u32,
    pagesz: u32,
    offset: u64,
    bins: Vec<VecDeque<u64>>,
}

impl Bin {
    /// binsz: The object size in each bin are multiples of binsz
    /// max: The total size of the range of memory. range is 0 to max
    /// pagesz: If we need more objects in a bin, we allocate a minimum of pagesz/object-size
    pub fn new(mut binsz: u32, max: u64, pagesz: u32) -> Bin {
        binsz = pow2_u32(binsz as u32);
        Bin {
            binsz,
            max,
            zeroes: binsz.leading_zeros(),
            pagesz: pow2_u32(pagesz as u32),
            offset: 0,
            bins: Vec::new(),
        }
    }

    // The Bin::bins[index] into which an object of 'size' will fit into
    fn index(&self, size: u32) -> (u32, usize) {
        let size = pow2_u32(size as u32);
        let size = cmp::max(size, self.binsz);
        let index = self.zeroes - size.leading_zeros();
        (size, index as usize)
    }

    // Add more objects to a bin
    fn resize(&mut self, size: u32, index: usize) {
        let alloc = if size > self.pagesz {
            if size % self.pagesz != 0 {
                self.pagesz * size / self.pagesz + 1
            } else {
                self.pagesz * size / self.pagesz
            }
        } else {
            self.pagesz
        };
        if alloc as u64 + self.offset <= self.max {
            let mut i = 0;
            while i < alloc {
                self.bins[index].push_front(self.offset);
                i += size;
                self.offset += size as u64;
            }
        }
    }

    // Get a 0 based offset into the range, for an object of size 'size'
    pub fn get(&mut self, size: u32) -> Option<u64> {
        if size == 0 {
            return None;
        }
        let (size, index) = self.index(size);
        if index >= self.bins.len() {
            self.bins.resize(index + 1, VecDeque::new())
        }
        if let Some(val) = self.bins[index].pop_front() {
            Some(val)
        } else {
            self.resize(size, index);
            self.bins[index].pop_back()
        }
    }

    // Free object of size 'size' at offset 'base' into the proper bin
    pub fn free(&mut self, base: u64, size: u32) {
        if size == 0 {
            panic!("Bad bin free, base {}, size {}", base, size);
        }
        let (size, index) = self.index(size);
        if index >= self.bins.len() {
            panic!(
                "Bad bin free, base {}, size {}, index {}",
                base, size, index
            );
        }
        self.bins[index].push_front(base);
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bin() {
        let mut bin = Bin::new(4, 32, 8);
        assert_eq!(bin.zeroes, 29);
        // Get a larger than possible bin
        let cntr = bin.get(33);
        assert!(cntr.is_none());
        // Get one 4 byte bin, it allocates two 4 byte bins (pagesize 8)
        let cntr = bin.get(4).unwrap();
        assert_eq!(cntr, 0);
        assert_eq!(bin.offset(), 8);
        assert_eq!(bin.bins[0].len(), 1);
        bin.free(cntr, 4);
        // Free the 4 byte bin
        assert_eq!(bin.bins[0].len(), 2);
        // Get two 4 byte bins, its already allocated
        let cntr1 = bin.get(4).unwrap();
        let cntr2 = bin.get(4).unwrap();
        assert_eq!(cntr1, 0);
        assert_eq!(cntr2, 4);
        assert_eq!(bin.offset(), 8);
        assert_eq!(bin.bins[0].len(), 0);
        // Get one 16 byte bin (power of two 13)
        let cntr = bin.get(13).unwrap();
        assert_eq!(cntr, 8);
        assert_eq!(bin.offset(), 24);
        assert_eq!(bin.bins[2].len(), 0);
        bin.free(cntr, 13);
        assert_eq!(bin.bins[2].len(), 1);
        // Get one 8 byte bin (power of two 5)
        let cntr = bin.get(5).unwrap();
        assert_eq!(cntr, 24);
        assert_eq!(bin.offset(), 32);
        assert_eq!(bin.bins[1].len(), 0);
        bin.free(cntr, 5);
        assert_eq!(bin.bins[1].len(), 1);
        // get one more 4 byte value (pow of 2 of 3)
        let cntr = bin.get(3);
        assert!(cntr.is_none());
    }
}
