use std::{
    cell::RefCell,
    net::{Ipv4Addr, Ipv6Addr},
    rc::Rc,
};

#[cfg(test)]
mod tests;

fn write_varint(bytes: &mut Vec<u8>, mut v: usize) -> usize {
    if v == 0 {
        bytes.push(0);
        return 1;
    }

    let mut len = 0;
    while v > 0 {
        let b = if v > 0x7f { (v & 0x7f) | 0x80 } else { v };
        bytes.push(b as u8);
        v >>= 7;
        len += 1;
    }
    len
}

fn read_varint(bytes: &[u8]) -> (usize, usize) {
    let mut v = 0;
    let mut pos = 0;
    while bytes[pos] & 0x80 != 0 {
        v |= ((bytes[pos] & 0x7f) as usize) << (pos * 7);
        pos += 1;
    }
    (v | (bytes[pos] as usize) << (pos * 7), pos + 1)
}

pub type IpSetV4 = IpSet<32>;
pub type IpSetV6 = IpSet<128>;

pub struct IpSet<const BITS: usize> {
    nodes: Vec<u8>,
}

impl<const BITS: usize> IpSet<BITS> {
    fn load_node_refs(&self, offset: usize) -> (usize, usize) {
        let (left_off, n) = read_varint(&self.nodes[offset..]);
        let left_ref_to = if left_off != 0 { offset + left_off } else { 0 };

        let (right_off, _) = read_varint(&self.nodes[offset + n..]);
        let right_ref_to = if right_off != 0 { offset + n + right_off } else { 0 };

        (left_ref_to, right_ref_to)
    }

    #[inline]
    fn contains_with(&self, bits: impl IntoIterator<Item = bool>) -> bool {
        let mut bits = bits.into_iter().take(BITS);

        let mut offset = 0;

        while let Some(next_node) = bits.next() {
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
    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        let mut bits = addr.to_bits();
        self.contains_with((0..32).map(|_| {
            let ret = bits & (1 << 31) != 0;
            bits = bits << 1;
            ret
        }))
    }
}

impl<const BITS: usize> IpSet<BITS> {
    pub fn builder() -> IpSetBuilder<BITS> {
        IpSetBuilder::new()
    }
}

#[derive(PartialEq)]
enum Node {
    Matched,
    Building { left: Option<NodeRef>, right: Option<NodeRef> },
}

type NodeRef = Rc<RefCell<Node>>;

impl Node {
    fn new_empty_ref() -> NodeRef {
        Rc::new(RefCell::new(Node::Building { left: None, right: None }))
    }
}

pub type IpSetBuilderV4 = IpSetBuilder<32>;
pub type IpSetBuilderV6 = IpSetBuilder<128>;

pub struct IpSetBuilder<const BITS: usize> {
    root: NodeRef,
}

impl<const BITS: usize> IpSetBuilder<BITS> {
    fn new() -> Self {
        Self {
            root: Node::new_empty_ref(),
        }
    }

    #[inline]
    fn add_with(&mut self, bits: impl IntoIterator<Item = bool>) {
        let mut bits = bits.into_iter().take(BITS);

        let mut node = self.root.clone();

        while let Some(next_node) = bits.next() {
            let next_node = match &mut *node.borrow_mut() {
                Node::Matched => return,
                Node::Building { left, right } => {
                    let next_node = match next_node {
                        false => left,
                        true => right,
                    };

                    match next_node {
                        None => next_node.insert(Node::new_empty_ref()).clone(),
                        Some(r) => r.clone(),
                    }
                }
            };
            node = next_node;
        }

        *node.borrow_mut() = Node::Matched;
    }
}

impl IpSetBuilder<128> {
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

impl<const BITS: usize> IpSetBuilder<BITS> {
    fn build_nodes_ref_index(self) -> Vec<usize> {
        fn push_node(nodes: &mut Vec<usize>, node_ref: &NodeRef) {
            let current_node_index = nodes.len();

            nodes.extend_from_slice(&[0, 0]);

            match &*node_ref.borrow() {
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

    pub fn build(self) -> IpSet<BITS> {
        const MAX_PASS: usize = 16;

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

            // Step 1: generate bytes seq and update offset of node
            for (idx, value) in nodes_value.iter().enumerate() {
                nodes_ref_offset[idx] = bytes.len();
                write_varint(&mut bytes, *value);
            }

            // Step 2: update nodes values
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
