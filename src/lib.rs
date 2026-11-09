#![doc = include_str!("../README.md")]

use std::net::{Ipv4Addr, Ipv6Addr};

// Encode an unsigned offset in little-endian groups of seven payload bits.
fn write_varint(bytes: &mut Vec<u8>, mut v: usize) -> usize {
    let mut len = 0;
    while v > 0x7f {
        bytes.push(((v & 0x7f) | 0x80) as u8);
        v >>= 7;
        len += 1;
    }
    bytes.push(v as u8);
    len + 1
}

// Only used with the internal encoding produced by build(), not untrusted bytes.
fn read_varint(bytes: &[u8]) -> (usize, usize) {
    let mut v = 0;
    let mut pos = 0;
    while bytes[pos] & 0x80 != 0 {
        v |= ((bytes[pos] & 0x7f) as usize) << (pos * 7);
        pos += 1;
    }
    (v | (bytes[pos] as usize) << (pos * 7), pos + 1)
}

/// An immutable IPv4 prefix set. Construct one with [`IpSetV4::builder`].
pub type IpSetV4 = IpSet<32>;
/// An immutable IPv6 prefix set. Construct one with [`IpSetV6::builder`].
pub type IpSetV6 = IpSet<128>;

/// An immutable union of prefixes over `BITS`-bit values.
///
/// `BITS` can range from 0 through 255. Use [`IpSetV4`] and [`IpSetV6`] for IP
/// addresses, or [`Self::contains_with`] for custom bit sequences. An empty set
/// matches nothing; a set containing the empty prefix matches every valid input.
///
/// The set stores a packed binary trie and performs allocation-free queries in
/// at most `BITS` trie levels. The builder uses additional temporary memory.
pub struct IpSet<const BITS: u8> {
    nodes: Vec<u8>,
}

impl<const BITS: u8> IpSet<BITS> {
    fn load_node_refs(&self, offset: usize) -> (usize, usize) {
        // Each child offset is relative to the start of its own varint, not the node.
        let (left_off, n) = read_varint(&self.nodes[offset..]);
        let left_ref_to = if left_off != 0 { offset + left_off } else { 0 };

        let (right_off, _) = read_varint(&self.nodes[offset + n..]);
        let right_ref_to = if right_off != 0 { offset + n + right_off } else { 0 };

        (left_ref_to, right_ref_to)
    }

    /// Queries an address represented by exactly `BITS` bits, most significant bit first.
    ///
    /// If the iterator reports an exact length through `size_hint()`, that length is
    /// checked before querying and the query may return without consuming all bits.
    /// Otherwise, length validation continues even after the query result is known,
    /// consuming at most `BITS + 1` bits. The first `None` ends the input sequence.
    ///
    /// # Panics
    ///
    /// Panics if the input contains fewer or more than `BITS` bits, even for an empty
    /// set or a set that matches every address.
    #[inline]
    pub fn contains_with(&self, bits: impl IntoIterator<Item = bool>) -> bool {
        struct BitsGuard<const BITS: u8, I> {
            count: usize,
            bits: Option<I>,
        }

        impl<const BITS: u8, I> Iterator for BitsGuard<BITS, I>
        where
            I: Iterator<Item = bool>,
        {
            type Item = bool;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                match self.bits.as_mut().and_then(|bits| bits.next()) {
                    Some(next_bit) => {
                        if self.count == usize::from(BITS) {
                            panic!("contains_with requires exactly {BITS} bits");
                        }

                        self.count += 1;

                        Some(next_bit)
                    }
                    None => {
                        if self.count != usize::from(BITS) {
                            panic!("contains_with requires exactly {BITS} bits");
                        }

                        // Remember exhaustion even if the underlying iterator is not fused.
                        let _ = self.bits.take();

                        None
                    }
                }
            }
        }

        let bits = bits.into_iter();
        let (lower, upper) = bits.size_hint();
        if upper == Some(lower) {
            assert_eq!(lower, usize::from(BITS), "contains_with requires exactly {BITS} bits");
            return self.contains_bits(bits.take(usize::from(BITS)));
        }

        let mut guard = BitsGuard::<BITS, _> {
            count: 0,
            bits: Some(bits),
        };
        let ret = self.contains_bits(&mut guard);
        // Early membership results must not skip validation of the remaining input.
        guard.for_each(|_| {});
        ret
    }

    #[inline]
    fn contains_bits(&self, bits: impl Iterator<Item = bool>) -> bool {
        if self.nodes.is_empty() {
            return false;
        }

        let mut offset = 0;

        for next_node in bits {
            let (left_node_ref, right_node_ref) = self.load_node_refs(offset);
            if left_node_ref == 0 && right_node_ref == 0 {
                return true;
            }

            offset = match next_node {
                false if left_node_ref != 0 => left_node_ref,
                true if right_node_ref != 0 => right_node_ref,
                _ => return false,
            };
        }

        true
    }
}

impl IpSet<128> {
    /// Returns whether `addr` belongs to any IPv6 prefix in the set.
    ///
    /// IPv4-mapped addresses are treated as IPv6 addresses without conversion.
    pub fn contains(&self, addr: Ipv6Addr) -> bool {
        let mut bits = addr.to_bits();
        self.contains_with((0..128).map(|_| {
            let ret = bits & (1 << 127) != 0;
            bits = bits << 1;
            ret
        }))
    }
}

impl IpSet<32> {
    /// Returns whether `addr` belongs to any IPv4 prefix in the set.
    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        let mut bits = addr.to_bits();
        self.contains_with((0..32).map(|_| {
            let ret = bits & (1 << 31) != 0;
            bits = bits << 1;
            ret
        }))
    }
}

impl<const BITS: u8> IpSet<BITS> {
    /// Creates an empty builder for this bit width.
    ///
    /// Building it without adding any prefixes produces an empty set.
    pub fn builder() -> IpSetBuilder<BITS> {
        IpSetBuilder::new()
    }
}

#[derive(PartialEq)]
enum Node {
    Matched,
    Building {
        left: Option<Box<Node>>,
        right: Option<Box<Node>>,
    },
}

impl Node {
    const EMPTY: Node = Node::Building { left: None, right: None };
}

/// A builder for [`IpSetV4`], created by [`IpSetV4::builder`].
pub type IpSetBuilderV4 = IpSetBuilder<32>;
/// A builder for [`IpSetV6`], created by [`IpSetV6::builder`].
pub type IpSetBuilderV6 = IpSetBuilder<128>;

/// Collects prefixes before consuming them to build a compact [`IpSet`].
///
/// Create a builder with [`IpSet::builder`]. Prefixes are combined as a union;
/// duplicates, overlaps, and insertion order do not affect membership. Adding a
/// broader prefix discards any more-specific prefixes below it in the builder.
pub struct IpSetBuilder<const BITS: u8> {
    root: Node,
}

impl<const BITS: u8> IpSetBuilder<BITS> {
    fn new() -> Self {
        Self { root: Node::EMPTY }
    }

    /// Adds a prefix given as bits in most-significant-first order.
    ///
    /// At most `BITS` bits are used; additional bits are ignored. The first `None`
    /// ends the prefix. An empty sequence adds the universal prefix. If an existing
    /// prefix already covers the input, this method may return without consuming
    /// the rest of the iterator.
    ///
    /// Unlike [`IpSet::contains_with`], this method accepts partial bit sequences.
    #[inline]
    pub fn add_with(&mut self, bits: impl IntoIterator<Item = bool>) {
        let mut bits = bits.into_iter().take(usize::from(BITS));

        let mut node = &mut self.root;

        while let Some(next_node) = bits.next() {
            let next_node = match node {
                Node::Matched => return,
                Node::Building { left, right } => {
                    let next_node = match next_node {
                        false => left,
                        true => right,
                    };

                    match next_node {
                        None => next_node.insert(Box::new(Node::EMPTY)),
                        Some(r) => r,
                    }
                }
            };
            node = next_node;
        }

        *node = Node::Matched;
    }
}

impl IpSetBuilder<128> {
    /// Adds the IPv6 network identified by the first `prefix` bits of `addr`.
    ///
    /// Host bits are ignored. A prefix length of 0 matches all IPv6 addresses;
    /// 128 adds a single address.
    ///
    /// # Panics
    ///
    /// Panics if `prefix` is greater than 128.
    pub fn add(&mut self, addr: Ipv6Addr, prefix: u8) {
        assert!(prefix <= 128);

        let mut bits = addr.to_bits();
        self.add_with((0..prefix).map(|_| {
            let ret = bits & (1 << 127) != 0;
            bits = bits << 1;
            ret
        }))
    }
}

impl IpSetBuilder<32> {
    /// Adds the IPv4 network identified by the first `prefix` bits of `addr`.
    ///
    /// Host bits are ignored. A prefix length of 0 matches all IPv4 addresses;
    /// 32 adds a single address.
    ///
    /// # Panics
    ///
    /// Panics if `prefix` is greater than 32.
    pub fn add(&mut self, addr: Ipv4Addr, prefix: u8) {
        assert!(prefix <= 32);

        let mut bits = addr.to_bits();
        self.add_with((0..prefix).map(|_| {
            let ret = bits & (1 << 31) != 0;
            bits = bits << 1;
            ret
        }))
    }
}

impl<const BITS: u8> IpSetBuilder<BITS> {
    fn build_nodes_ref_index(self) -> Vec<usize> {
        fn push_node(nodes: &mut Vec<usize>, node: &Node) {
            let current_node_index = nodes.len();

            nodes.extend_from_slice(&[0, 0]);

            match node {
                Node::Matched => {}
                Node::Building { left, right } => {
                    let [left_node_index, right_node_index] = [left, right].map(|r| match r {
                        None => 0,
                        Some(r) => {
                            let index = nodes.len();
                            push_node(&mut *nodes, r);
                            index
                        }
                    });
                    nodes[current_node_index] = left_node_index;
                    nodes[current_node_index + 1] = right_node_index;
                }
            }
        }

        let mut ref_index = Vec::<usize>::with_capacity(4096);

        push_node(&mut ref_index, &self.root);

        ref_index
    }

    /// Consumes the builder and packs its prefixes into an immutable set.
    ///
    /// If no prefixes were added, the result matches nothing. Building uses
    /// temporary indexing buffers in addition to the trie, so peak memory usage
    /// can exceed the final encoded size. Node storage is shrunk before returning.
    ///
    /// With the `trace` feature enabled, nonempty builds emit `tracing` events
    /// describing compression passes and the final encoded size.
    ///
    /// # Panics
    ///
    /// Panics if relative-offset encoding does not converge within 16 passes.
    pub fn build(self) -> IpSet<BITS> {
        const MAX_PASS: usize = 16;

        // Empty sets have no nodes; [0, 0] is reserved for a matched node.
        if matches!(self.root, Node::Building { left: None, right: None }) {
            return IpSet { nodes: Vec::new() };
        }

        let nodes_ref_index = self.build_nodes_ref_index();

        let mut nodes_ref_offset = (0..nodes_ref_index.len())
            .map(|idx| idx * size_of::<usize>())
            .collect::<Vec<_>>();
        let mut nodes_value = nodes_ref_index
            .iter()
            .enumerate()
            .map(|(idx, r)| if *r != 0 { (*r - idx) * size_of::<usize>() } else { 0 })
            .collect::<Vec<_>>();
        let mut bytes = Vec::<u8>::with_capacity(4096);

        let mut has_changed = true;
        let mut pass_count = 0;
        while has_changed && pass_count < MAX_PASS {
            has_changed = false;
            pass_count += 1;

            bytes.clear();

            // Encode the current offsets and record each varint's byte position.
            for (idx, value) in nodes_value.iter().enumerate() {
                nodes_ref_offset[idx] = bytes.len();
                write_varint(&mut bytes, *value);
            }

            // Recompute relative offsets; changed varint widths may require another pass.
            for (idx, ref_idx) in nodes_ref_index.iter().enumerate() {
                let value = &mut nodes_value[idx];
                let new_value = if *ref_idx != 0 {
                    nodes_ref_offset[*ref_idx] - nodes_ref_offset[idx]
                } else {
                    0
                };

                if *value != new_value {
                    *value = new_value;
                    has_changed = true;
                }
            }

            #[cfg(feature = "trace")]
            tracing::trace!(pass_count, has_changed, len = bytes.len(), "building ipset")
        }

        #[cfg(feature = "trace")]
        tracing::debug!(pass_count, has_changed, len = bytes.len(), "ipset built");

        assert!(!has_changed);

        bytes.shrink_to_fit();

        IpSet { nodes: bytes }
    }
}

#[cfg(test)]
mod tests;
