// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! LZCOMP decompression (MicroType Express, section 3 and Appendix C).
//!
//! An LZ77 variant whose literals, copy lengths and distances are coded
//! with three adaptive Huffman trees. This follows the reference decoder
//! bit for bit: the trees' initial weights, the update and swap rules and
//! the preloaded history all change the decoded output.

use crate::Error;

/// Reads bits most significant first, as `MTX_BITIO_input_bit`.
struct BitReader<'a> {
    data: &'a [u8],
    index: usize,
    /// Bits not read yet, the next one highest.
    buffer: u64,
    count: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, index: 0, buffer: 0, count: 0 }
    }

    #[inline]
    fn bit(&mut self) -> Result<bool, Error> {
        if self.count == 0 {
            self.refill()?;
        }
        self.count -= 1;
        let bit = self.buffer >> 63 != 0;
        self.buffer <<= 1;
        Ok(bit)
    }

    /// Take up to eight more bytes into the buffer.
    #[cold]
    fn refill(&mut self) -> Result<(), Error> {
        let rest = &self.data[self.index.min(self.data.len())..];
        if rest.is_empty() {
            return Err(Error("LZCOMP のデータが途中で終わっています"));
        }
        let n = rest.len().min(8);
        let mut bytes = [0u8; 8];
        bytes[..n].copy_from_slice(&rest[..n]);
        self.buffer = u64::from_be_bytes(bytes);
        self.count = 8 * n as u32;
        self.index += n;
        Ok(())
    }

    fn value(&mut self, bits: u32) -> Result<u32, Error> {
        let mut v = 0;
        for _ in 0..bits {
            v = (v << 1) | self.bit()? as u32;
        }
        Ok(v)
    }
}

/// An adaptive Huffman tree over symbols `0..range` (`AHUFF`).
///
/// Nodes are numbered from 1 (the root); node 0 is a sentinel. Children of
/// node `i` start as `2i` and `2i + 1`, leaves are `range..2 * range`.
struct AdaptiveHuffman {
    /// Weights by node, apart from the rest: `update` scans them.
    weight: Vec<u32>,
    nodes: Vec<Node>,
    symbol_index: Vec<u32>,
}

/// A node's place in the tree. A leaf has its symbol in `code`, an internal
/// node `-1`.
#[derive(Clone, Copy)]
struct Node {
    up: u32,
    left: u32,
    right: u32,
    code: i32,
}

const ROOT: usize = 1;

impl AdaptiveHuffman {
    fn new(range: usize) -> Self {
        let n = 2 * range;
        let mut t = Self {
            weight: vec![0; n],
            nodes: vec![Node { up: 0, left: 0, right: 0, code: -1 }; n],
            symbol_index: vec![0; range],
        };
        for i in 2..n {
            t.nodes[i].up = (i / 2) as u32;
            t.weight[i] = 1;
        }
        for i in 1..range {
            t.nodes[i].left = (2 * i) as u32;
            t.nodes[i].right = (2 * i + 1) as u32;
        }
        for i in 0..range {
            t.nodes[range + i].code = i as i32;
            t.symbol_index[i] = (range + i) as u32;
        }
        // Internal weights are the sums of their children's; children have
        // higher numbers than their parents at this point.
        for a in (ROOT..range).rev() {
            t.weight[a] = t.weight[t.nodes[a].left as usize] + t.weight[t.nodes[a].right as usize];
        }
        // The sentinel stops the sibling search in `update`.
        t.weight[0] = u32::MAX;

        if range > 256 && range < 512 {
            // The symbol tree: favour the first two copy symbols and the
            // single byte copies DUP2 and DUP4.
            t.update(t.symbol_index[256] as usize);
            t.update(t.symbol_index[257] as usize);
            for _ in 0..12 {
                t.update(t.symbol_index[range - 3] as usize);
            }
            for _ in 0..6 {
                t.update(t.symbol_index[range - 2] as usize);
            }
        } else {
            for _ in 0..2 {
                for i in 0..range {
                    t.update(t.symbol_index[i] as usize);
                }
            }
        }
        t
    }

    fn swap(&mut self, a: usize, b: usize) {
        let (upa, upb) = (self.nodes[a].up, self.nodes[b].up);
        self.nodes.swap(a, b);
        self.weight.swap(a, b);
        self.nodes[a].up = upa;
        self.nodes[b].up = upb;
        for node in [a, b] {
            let Node { left, right, code, .. } = self.nodes[node];
            if code < 0 {
                self.nodes[left as usize].up = node as u32;
                self.nodes[right as usize].up = node as u32;
            } else {
                self.symbol_index[code as usize] = node as u32;
            }
        }
    }

    /// Add one to the weight of node `a` and its ancestors, keeping nodes
    /// in decreasing weight order (`UpdateWeight`).
    fn update(&mut self, mut a: usize) {
        while a != ROOT {
            let weight = self.weight[a];
            let mut b = a - 1;
            if self.weight[b] == weight {
                while self.weight[b] == weight {
                    b -= 1;
                }
                b += 1;
                if b > ROOT {
                    self.swap(a, b);
                    a = b;
                }
            }
            self.weight[a] = weight + 1;
            a = self.nodes[a].up as usize;
        }
        self.weight[ROOT] += 1;
    }

    #[inline]
    fn read(&mut self, bits: &mut BitReader) -> Result<usize, Error> {
        let mut a = ROOT;
        let code = loop {
            let node = self.nodes[a];
            a = if bits.bit()? { node.right } else { node.left } as usize;
            let code = self.nodes[a].code;
            if code >= 0 {
                break code;
            }
        };
        // Take the symbol first: updating may move the leaf elsewhere.
        self.update(a);
        Ok(code as usize)
    }
}

/// Undoes the run-length pass (`MTX_RUNLENGTHCOMP_SaveBytes`). The first
/// byte is the escape; `escape, 0` is a literal escape and `escape, n, b`
/// is `n` copies of `b`.
enum RunLength {
    Initial,
    Normal { escape: u8 },
    SeenEscape { escape: u8 },
    NeedByte { escape: u8, count: u8 },
}

impl RunLength {
    fn push(&mut self, value: u8, out: &mut Vec<u8>) {
        *self = match *self {
            RunLength::Initial => RunLength::Normal { escape: value },
            RunLength::Normal { escape } if value == escape => RunLength::SeenEscape { escape },
            RunLength::Normal { escape } => {
                out.push(value);
                RunLength::Normal { escape }
            }
            RunLength::SeenEscape { escape } if value == 0 => {
                out.push(escape);
                RunLength::Normal { escape }
            }
            RunLength::SeenEscape { escape } => RunLength::NeedByte { escape, count: value },
            RunLength::NeedByte { escape, count } => {
                out.extend(std::iter::repeat_n(value, count as usize));
                RunLength::Normal { escape }
            }
        }
    }
}

/// History preloaded before the data, so early bytes can use copies.
fn preload() -> Vec<u8> {
    let mut h = Vec::with_capacity(PRELOAD_SIZE);
    for k in 0..32u8 {
        for j in 0..96u8 {
            h.push(k);
            h.push(j);
        }
    }
    for j in 0..=255u8 {
        h.extend([j; 4]);
    }
    h
}

const PRELOAD_SIZE: usize = 2 * 32 * 96 + 4 * 256;
const MAX_2BYTE_DIST: usize = 512;

/// Decompress one LZCOMP stream.
pub(crate) fn decompress(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut bits = BitReader::new(data);
    let using_run_length = bits.bit()?;
    let mut dist = AdaptiveHuffman::new(8);
    let mut len = AdaptiveHuffman::new(8);
    let out_len = bits.value(24)? as usize;

    // Enough 3-bit distance groups to reach back over the whole output.
    let mut dist_ranges = 1u32;
    while (1usize << (3 * dist_ranges)) < out_len {
        dist_ranges += 1;
    }
    let dup2 = 256 + 8 * dist_ranges as usize;
    let (dup4, dup6) = (dup2 + 1, dup2 + 2);
    let mut sym = AdaptiveHuffman::new(dup6 + 1);

    let mut history = preload();
    history.reserve(out_len);
    let mut out = Vec::with_capacity(out_len);
    let mut run_length = RunLength::Initial;
    let mut emit = |value: u8, history: &mut Vec<u8>, out: &mut Vec<u8>| {
        history.push(value);
        if using_run_length {
            run_length.push(value, out);
        } else {
            out.push(value);
        }
    };

    let mut pos = 0;
    while pos < out_len {
        let symbol = sym.read(&mut bits)?;
        let back = |n: usize, history: &Vec<u8>| history[history.len() - n];
        if symbol < 256 {
            emit(symbol as u8, &mut history, &mut out);
            pos += 1;
        } else if symbol == dup2 || symbol == dup4 || symbol == dup6 {
            let n = 2 + 2 * (symbol - dup2);
            let value = back(n, &history);
            emit(value, &mut history, &mut out);
            pos += 1;
        } else {
            // Length: 2 bits per symbol, the third bit set while more follow.
            let first = symbol - 256;
            let ranges = first / 8 + 1;
            let mut bits3 = first % 8;
            let mut length = 0usize;
            loop {
                length = (length << 2) | (bits3 & 3);
                if bits3 & 4 == 0 {
                    break;
                }
                bits3 = len.read(&mut bits)?;
            }
            length += 2;
            let mut distance = 0usize;
            for _ in 0..ranges {
                distance = (distance << 3) | dist.read(&mut bits)?;
            }
            distance += 1;
            if distance >= MAX_2BYTE_DIST {
                length += 1;
            }
            // `distance` counts back to the last byte of the copied string.
            let from_end = distance + length - 1;
            if from_end > history.len() {
                return Err(Error("LZCOMP のコピー位置が範囲外です"));
            }
            let start = history.len() - from_end;
            for j in 0..length {
                let value = history[start + j];
                emit(value, &mut history, &mut out);
            }
            pos += length;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_has_the_reference_size() {
        assert_eq!(preload().len(), PRELOAD_SIZE);
        assert_eq!(&preload()[..4], &[0, 0, 0, 1]);
    }

    #[test]
    fn run_length_escapes() {
        let mut rl = RunLength::Initial;
        let mut out = Vec::new();
        // Escape 0xAA; "1, AA 0, 2, AA 3 7" is 1, AA, 2, 7, 7, 7.
        for b in [0xAA, 1, 0xAA, 0, 2, 0xAA, 3, 7] {
            rl.push(b, &mut out);
        }
        assert_eq!(out, vec![1, 0xAA, 2, 7, 7, 7]);
    }

    /// The reference `check_tree`: parents and children agree, weights sum
    /// up and never increase along the node numbering.
    fn check(t: &AdaptiveHuffman, range: usize) {
        for i in ROOT..range * 2 {
            let Node { left, right, code, .. } = t.nodes[i];
            let (left, right) = (left as usize, right as usize);
            if code < 0 {
                assert_eq!(t.nodes[left].up as usize, i, "left child of {i}");
                assert_eq!(t.nodes[right].up as usize, i, "right child of {i}");
                assert_eq!(t.weight[i], t.weight[left] + t.weight[right], "weight of {i}");
            } else {
                assert_eq!(t.symbol_index[code as usize] as usize, i, "index of leaf {i}");
            }
            if i > ROOT {
                assert!(t.weight[i - 1] >= t.weight[i], "order at {i}");
            }
        }
    }

    #[test]
    fn huffman_trees_stay_valid() {
        for range in [8, 307] {
            let mut t = AdaptiveHuffman::new(range);
            check(&t, range);
            let mut seed = 12345u32;
            for _ in 0..5000 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let symbol = (seed >> 16) as usize % range;
                t.update(t.symbol_index[symbol] as usize);
            }
            check(&t, range);
        }
    }

    #[test]
    fn huffman_trees_start_valid() {
        for range in [8, 315] {
            let t = AdaptiveHuffman::new(range);
            for i in ROOT..2 * range {
                let Node { left, right, code, .. } = t.nodes[i];
                if code < 0 && left != 0 {
                    assert_eq!(t.weight[i], t.weight[left as usize] + t.weight[right as usize], "node {i}");
                }
                if i > ROOT {
                    assert!(t.weight[i - 1] >= t.weight[i], "order at {i}");
                }
            }
        }
    }
}
