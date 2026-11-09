# compact-ipset

Compact, immutable IPv4 and IPv6 prefix sets for in-memory membership queries.
Build a set from network prefixes, then query whether an address belongs to any
of them. This library does not manage operating-system firewall or ipset tables.

[Crate](https://crates.io/crates/compact-ipset) ·
[API documentation](https://docs.rs/compact-ipset) ·
[Repository](https://github.com/Kr328/ipset-rs)

## Installation

```toml
[dependencies]
compact-ipset = "1"
```

The Rust import name is `compact_ipset`. Rust 1.85 or newer is required.

## IPv4

```rust
use compact_ipset::IpSetV4;

let mut builder = IpSetV4::builder();
builder.add("10.0.0.0".parse().unwrap(), 8);
builder.add("192.168.1.0".parse().unwrap(), 24);
builder.add("203.0.113.7".parse().unwrap(), 32);

let set = builder.build();
assert!(set.contains("10.1.2.3".parse().unwrap()));
assert!(set.contains("192.168.1.255".parse().unwrap()));
assert!(set.contains("203.0.113.7".parse().unwrap()));
assert!(!set.contains("192.168.2.1".parse().unwrap()));
assert!(!set.contains("203.0.113.8".parse().unwrap()));
```

## IPv6

```rust
use compact_ipset::IpSetV6;

let mut builder = IpSetV6::builder();
builder.add("2001:db8::".parse().unwrap(), 32);
builder.add("::1".parse().unwrap(), 128);

let set = builder.build();
assert!(set.contains("2001:db8:1234::1".parse().unwrap()));
assert!(set.contains("::1".parse().unwrap()));
assert!(!set.contains("2001:db9::1".parse().unwrap()));
```

## Semantics and storage

- Prefix lengths are `0..=32` for IPv4 and `0..=128` for IPv6. An invalid prefix
  length panics. Bits outside the prefix are ignored, so `192.168.1.42/24` adds
  the same network as `192.168.1.0/24`.
- Prefixes form a union. Duplicate and overlapping prefixes are allowed, and
  insertion order does not change membership.
- A builder with no prefixes produces an empty set. An explicit `/0` prefix
  matches every address in its address family.
- `build()` consumes the builder. The resulting set supports membership queries;
  it does not support insertion, removal, or enumeration of stored prefixes.
- IPv4 and IPv6 sets are separate. IPv4-mapped IPv6 addresses are queried as IPv6
  addresses, without automatic conversion to IPv4.
- Building uses a heap-allocated binary trie and temporary indexing buffers. The
  finished set stores nodes and variable-length relative offsets in a byte vector.
  Peak build memory can be substantially larger than the finished set.
- An IP query visits at most 32 or 128 trie levels, regardless of the number of
  inserted prefixes. Queries allocate no memory. Custom bit iterators may perform
  their own work or allocations.

## Custom bit sequences

`IpSet<BITS>` supports bit widths from 0 through 255. Supply bits most significant
bit first. `add_with()` adds a prefix of up to `BITS` bits: an empty sequence adds
the universal prefix, and input beyond `BITS` is ignored. It may return early if
an existing prefix already covers the input.

`contains_with()` requires **exactly `BITS` bits** and panics on shorter or longer
input, including for empty or universal sets. An iterator reporting an exact
`size_hint()` is checked before traversal and can be consumed only partially.
Otherwise, validation reads through the end or the first excess bit, even if the
membership result is already known; it consumes at most `BITS + 1` bits. The first
`None` ends the input sequence.

```rust
use compact_ipset::IpSet;

let mut builder = IpSet::<4>::builder();
builder.add_with([true, false]); // All four-bit values starting with 10.
let set = builder.build();

assert!(set.contains_with([true, false, true, true]));
assert!(!set.contains_with([true, true, false, false]));
```

For `IpSet<0>`, the empty sequence is the only valid query. It matches only after
an empty prefix has been explicitly added.

## Features

The default feature set is empty, with no runtime dependencies.

Enable `trace` for `tracing` events reporting compression passes and the final
encoded size during nonempty builds. The library does not install a subscriber.

```toml
[dependencies]
compact-ipset = { version = "1", features = ["trace"] }
```

## Development

The two `test_china_route*` tests download live routing data from GitHub. To run
the remaining unit tests and all documentation examples without those downloads:

```sh
cargo test --all-features -- --skip test_china_route
```

Add `--offline` to the Cargo command when dependencies are already cached.

## License

MIT. See [LICENSE](https://github.com/Kr328/ipset-rs/blob/main/LICENSE).
