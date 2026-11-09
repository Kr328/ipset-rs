use std::{
    net::Ipv6Addr,
    panic::{AssertUnwindSafe, catch_unwind},
    time::Instant,
};

use tracing_subscriber::fmt::format::FmtSpan;

use crate::{IpSet, IpSetV4, IpSetV6, read_varint};

#[test]
fn test_empty_ipsets() {
    let v4 = IpSetV4::builder().build();
    for addr in ["0.0.0.0", "127.0.0.1", "1.2.3.4", "255.255.255.255"] {
        assert!(!v4.contains(addr.parse().unwrap()), "{addr}");
    }
    assert!(catch_unwind(|| v4.contains_with([])).is_err());
    assert!(!v4.contains_with([false; 32]));
    assert!(!v4.contains_with([true; 32]));

    let v6 = IpSetV6::builder().build();
    for addr in ["::", "::1", "2001:db8::1", "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"] {
        assert!(!v6.contains(addr.parse().unwrap()), "{addr}");
    }
    assert!(catch_unwind(|| v6.contains_with([])).is_err());
    assert!(!v6.contains_with([false; 128]));
    assert!(!v6.contains_with([true; 128]));
}

#[test]
fn test_default_routes_and_empty_prefix() {
    for use_add_with in [false, true] {
        let mut v4 = IpSetV4::builder();
        let mut v6 = IpSetV6::builder();
        if use_add_with {
            v4.add_with([]);
            v6.add_with([]);
        } else {
            v4.add("0.0.0.0".parse().unwrap(), 0);
            v6.add("::".parse().unwrap(), 0);
        }

        let v4 = v4.build();
        for addr in ["0.0.0.0", "127.0.0.1", "1.2.3.4", "255.255.255.255"] {
            assert!(v4.contains(addr.parse().unwrap()), "{addr}, use_add_with={use_add_with}");
        }
        assert!(catch_unwind(|| v4.contains_with([])).is_err());
        assert!(v4.contains_with([false; 32]));
        assert!(v4.contains_with([true; 32]));

        let v6 = v6.build();
        for addr in ["::", "::1", "2001:db8::1", "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"] {
            assert!(v6.contains(addr.parse().unwrap()), "{addr}, use_add_with={use_add_with}");
        }
        assert!(catch_unwind(|| v6.contains_with([])).is_err());
        assert!(v6.contains_with([false; 128]));
        assert!(v6.contains_with([true; 128]));
    }
}

#[test]
fn test_zero_bit_ipset() {
    for matched in [false, true] {
        let mut builder = IpSet::<0>::builder();
        if matched {
            builder.add_with([]);
        }
        let set = builder.build();
        assert_eq!(set.contains_with([]), matched);
        assert_eq!(set.contains_with(std::iter::from_fn(|| None)), matched);
        assert!(catch_unwind(|| set.contains_with([false])).is_err());
        assert!(catch_unwind(|| set.contains_with(std::iter::from_fn(|| Some(false)))).is_err());
    }
}

#[test]
fn test_max_bit_ipset() {
    let mut builder = IpSet::<255>::builder();
    builder.add_with([false; 255]);
    let set = builder.build();

    let mut other = [false; 255];
    other[254] = true;
    assert!(!set.contains_with(other));

    for len in [254, 255, 256] {
        for unknown_size in [false, true] {
            let result = catch_unwind(|| {
                let mut bits = (0..len).map(|_| false);
                if unknown_size {
                    set.contains_with(std::iter::from_fn(move || bits.next()))
                } else {
                    set.contains_with(bits)
                }
            });
            if len == 255 {
                assert!(result.unwrap(), "unknown_size={unknown_size}");
            } else {
                assert!(result.is_err(), "len={len}, unknown_size={unknown_size}");
            }
        }
    }
}

#[test]
fn test_contains_with_bit_lengths() {
    let cases: [(&str, Option<&[bool]>, bool); 5] = [
        ("empty", None, false),
        ("full", Some(&[]), true),
        ("prefix match", Some(&[false, false]), true),
        ("prefix miss", Some(&[true]), false),
        ("leaf match", Some(&[false; 8]), true),
    ];
    for (name, prefix, expected) in cases {
        let mut builder = IpSet::<8>::builder();
        if let Some(prefix) = prefix {
            builder.add_with(prefix.iter().copied());
        }
        let set = builder.build();
        for len in [0, 1, 2, 7, 8, 9, 16] {
            for unknown_size in [false, true] {
                let result = catch_unwind(|| {
                    let mut bits = [false; 16][..len].iter().copied();
                    if unknown_size {
                        set.contains_with(std::iter::from_fn(move || bits.next()))
                    } else {
                        set.contains_with(bits)
                    }
                });
                if len == 8 {
                    assert_eq!(result.unwrap(), expected, "{name}, unknown_size={unknown_size}");
                } else {
                    assert!(result.is_err(), "{name}, len={len}, unknown_size={unknown_size}");
                }
            }
        }
    }
}

#[test]
fn test_contains_with_infinite_iterator_is_bounded() {
    let mut builder = IpSet::<8>::builder();
    builder.add_with([]);
    let set = builder.build();
    let mut reads = 0;
    let result = catch_unwind(AssertUnwindSafe(|| {
        set.contains_with(std::iter::from_fn(|| {
            reads += 1;
            assert!(reads <= 9, "read past the first excess bit");
            Some(false)
        }))
    }));
    assert!(result.is_err());
    assert_eq!(reads, 9);
}

#[test]
fn test_contains_with_stops_at_first_none() {
    let mut builder = IpSet::<8>::builder();
    builder.add_with([false; 8]);
    let set = builder.build();
    for len in [3, 8] {
        let mut reads = 0;
        let result = catch_unwind(AssertUnwindSafe(|| {
            set.contains_with(std::iter::from_fn(|| {
                reads += 1;
                if reads == len + 1 { None } else { Some(false) }
            }))
        }));
        assert_eq!(reads, len + 1);
        if len == 8 {
            assert!(result.unwrap());
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn test_contains_with_known_length_can_return_early() {
    let set = IpSet::<8>::builder().build();
    let mut reads = 0;
    let bits = [false; 8].into_iter().inspect(|_| reads += 1);
    assert!(!set.contains_with(bits));
    assert_eq!(reads, 0);

    let mut builder = IpSet::<8>::builder();
    builder.add_with([]);
    let set = builder.build();
    let bits = [false; 8].into_iter().inspect(|_| reads += 1);
    assert!(set.contains_with(bits));
    assert!(reads < 8);
}

#[test]
fn test_simple_ipset() {
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

    println!("{}", ipset.nodes.len());
}

#[tokio::test]
async fn test_china_route() {
    let _ = tracing_subscriber::fmt()
        .with_span_events(FmtSpan::CLOSE | FmtSpan::NEW)
        .with_max_level(tracing::Level::TRACE)
        .try_init();

    let data = reqwest::get("https://raw.githubusercontent.com/mayaxcn/china-ip-list/refs/heads/master/chnroute.txt")
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let insert_begin_at = Instant::now();

    let mut builder = IpSetV4::builder();

    for line in data.split("\n") {
        if line.is_empty() {
            continue;
        }

        let (addr, prefix_len) = line.split_once("/").unwrap();
        builder.add(addr.parse().unwrap(), prefix_len.parse().unwrap());
    }

    let ipset = builder.build();

    println!("insert time: {:?}", insert_begin_at.elapsed());
    println!("ipset len = {}", ipset.nodes.len());

    let mut index_sizes = [0; 8];
    let mut index_bit_max = 0;
    let mut index_bit_min = size_of::<usize>() * 8;
    let mut offset = 0;
    while offset < ipset.nodes.len() {
        let (index, n) = read_varint(&ipset.nodes[offset..]);
        index_sizes[n] += 1;
        offset += n;
        let index_bits = (size_of::<usize>() * 8) - index.leading_zeros() as usize;
        if index_bits > index_bit_max {
            index_bit_max = index_bits;
        }
        if index_bits < index_bit_min {
            index_bit_min = index_bits;
        }
    }

    println!(
        "ipset index_sizes = {:?} index_bit_max = {} index_bit_min = {}",
        index_sizes, index_bit_max, index_bit_min
    );

    let query_begin_at = Instant::now();
    for _ in 1..10000 {
        assert!(ipset.contains("119.29.29.29".parse().unwrap()));
        assert!(ipset.contains("223.5.5.5".parse().unwrap()));
        assert!(ipset.contains("114.114.114.114".parse().unwrap()));
        assert!(ipset.contains("157.148.134.13".parse().unwrap()));
        assert!(ipset.contains("157.148.134.12".parse().unwrap()));
        assert!(ipset.contains("157.148.134.11".parse().unwrap()));
        assert!(ipset.contains("157.148.69.186".parse().unwrap()));
        assert!(ipset.contains("157.148.69.151".parse().unwrap()));
        assert!(ipset.contains("106.11.249.99".parse().unwrap()));
        assert!(ipset.contains("106.11.172.9".parse().unwrap()));
        assert!(ipset.contains("140.205.60.46".parse().unwrap()));
        assert!(ipset.contains("106.11.248.146".parse().unwrap()));
        assert!(ipset.contains("106.11.253.83".parse().unwrap()));
        assert!(ipset.contains("61.241.54.232".parse().unwrap()));
        assert!(ipset.contains("61.241.54.211".parse().unwrap()));
        assert!(!ipset.contains("172.217.175.68".parse().unwrap()));
        assert!(!ipset.contains("91.108.56.1".parse().unwrap()));
        assert!(!ipset.contains("91.108.4.1".parse().unwrap()));
        assert!(!ipset.contains("91.108.8.1".parse().unwrap()));
        assert!(!ipset.contains("91.108.16.1".parse().unwrap()));
        assert!(!ipset.contains("91.108.12.1".parse().unwrap()));
        assert!(!ipset.contains("149.154.160.1".parse().unwrap()));
        assert!(!ipset.contains("91.105.192.1".parse().unwrap()));
        assert!(!ipset.contains("91.108.20.1".parse().unwrap()));
        assert!(!ipset.contains("185.76.151.1".parse().unwrap()));
    }
    println!("query time: {:?}", query_begin_at.elapsed() / 10000);
}

#[test]
fn test_ipv6_simple() {
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

    let begin_at = Instant::now();
    for _ in 1..10000 {
        for (addr, has) in test_addrs {
            assert_eq!(ipset.contains(addr), has);
        }
    }
    println!("query time: {:?}", begin_at.elapsed() / 10000);
}

#[tokio::test]
async fn test_china_route6() {
    let _ = tracing_subscriber::fmt()
        .with_span_events(FmtSpan::CLOSE | FmtSpan::NEW)
        .with_max_level(tracing::Level::TRACE)
        .try_init();

    let data = reqwest::get("https://raw.githubusercontent.com/mayaxcn/china-ip-list/refs/heads/master/chnroute_v6.txt")
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let insert_begin_at = Instant::now();

    let mut builder = IpSetV6::builder();

    for line in data.split("\n") {
        if line.is_empty() {
            continue;
        }

        let (addr, prefix_len) = line.split_once("/").unwrap();
        builder.add(addr.parse().unwrap(), prefix_len.parse().unwrap());
    }

    let ipset = builder.build();

    println!("insert time: {:?}", insert_begin_at.elapsed());

    println!("ipset.len = {}", ipset.nodes.len());

    let test_addrs: [(Ipv6Addr, bool); 6] = [
        ("2408:8756:d0fe:300::12".parse().unwrap(), true),
        ("2408:8756:d0fe:300::11".parse().unwrap(), true),
        ("2408:8756:d0fe:300::13".parse().unwrap(), true),
        ("2404:6800:4004:801::2004".parse().unwrap(), false),
        ("2606:4700::6810:85e5".parse().unwrap(), false),
        ("2606:4700::6810:84e5".parse().unwrap(), false),
    ];

    let query_begin_at = Instant::now();

    for _ in 1..10000 {
        for (addr, has) in test_addrs {
            assert_eq!(ipset.contains(addr), has);
        }
    }

    println!("query time: {:?}", query_begin_at.elapsed() / 10000);
}
