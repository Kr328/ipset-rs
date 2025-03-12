# ipset-rs

A compact ipset implement in Rust.

## Feature

* Small memory footprint.
* O(1) query time.

## Limit

* Only 'contains' operate is supported.

## Example

```rust
use ipset::IpSetV4;

fn simple_ipset_v4() {
    let mut builder = IpSetV4::builder();
    builder.add("10.0.0.0".parse().unwrap(), 8);
    builder.add("192.168.0.0".parse().unwrap(), 16);
    builder.add("192.168.0.0".parse().unwrap(), 24);
    builder.add("172.16.0.0".parse().unwrap(), 20);
    builder.add("1.2.3.4".parse().unwrap(), 32);

    let ipset = builder.build();
    assert!(ipset.contains("10.0.0.1".parse().unwrap()));
    assert!(ipset.contains("10.0.0.2".parse().unwrap()));
    assert!(ipset.contains("192.168.0.1".parse().unwrap()));
    assert!(ipset.contains("192.168.1.2".parse().unwrap()));
    assert!(ipset.contains("172.16.15.241".parse().unwrap()));
    assert!(ipset.contains("1.2.3.4".parse().unwrap()));
    assert!(!ipset.contains("172.16.255.241".parse().unwrap()));
    assert!(!ipset.contains("1.1.1.1".parse().unwrap()));
    assert!(!ipset.contains("1.0.0.1".parse().unwrap()));
    assert!(!ipset.contains("8.8.8.8".parse().unwrap()));
    assert!(!ipset.contains("8.8.4.4".parse().unwrap()));
    assert!(!ipset.contains("208.67.222.222".parse().unwrap()));
    assert!(!ipset.contains("208.67.220.220".parse().unwrap()));
    assert!(!ipset.contains("1.2.3.5".parse().unwrap()));
}

fn simple_ipset_v6() {
    let mut builder = IpSetV6::builder();
    builder.add("2001:db8::".parse().unwrap(), 32);
    builder.add("2001:db8::".parse().unwrap(), 64);
    builder.add("2002:db8::1".parse().unwrap(), 128);
    builder.add("2003:db8::1".parse().unwrap(), 64);
    builder.add("2004:db8::1".parse().unwrap(), 32);

    let ipset = builder.build();

    let test_addrs: [(Ipv6Addr, bool); 6] = [
        ("2001:db8::1".parse().unwrap(), true),
        ("2002:db8::1".parse().unwrap(), true),
        ("2003:db8::3".parse().unwrap(), true),
        ("2004:db8::4".parse().unwrap(), true),
        ("2005:db8::1".parse().unwrap(), false),
        ("2002:db8::2".parse().unwrap(), false),
    ];

    for (addr, has) in test_addrs {
        assert_eq!(ipset.contains(addr), has);
    }
}
```
