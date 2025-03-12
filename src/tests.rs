use std::{net::Ipv6Addr, time::Instant};

use tracing_subscriber::fmt::format::FmtSpan;

use crate::{IpSetV4, IpSetV6, read_varint};

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
