use super::*;
use libp2p_identity::PeerId;
use std::io;

#[test]
fn listen_addr() {
    let tcp_addr = "/ip4/0.0.0.0/tcp/2222".parse::<Multiaddr>().unwrap();

    // Check `/tls/ws`
    let addr = tcp_addr
        .clone()
        .with(Protocol::Tls)
        .with(Protocol::Ws("/".into()));
    let (inner_addr, proto) = parse_ws_listen_addr(&addr).unwrap();
    assert_eq!(&inner_addr, &tcp_addr);
    assert_eq!(proto, WsListenProto::TlsWs("/".into()));

    let mut listen_addr = tcp_addr.clone();
    proto.append_on_addr(&mut listen_addr);
    assert_eq!(listen_addr, addr);

    // Check `/wss`
    let addr = tcp_addr.clone().with(Protocol::Wss("/".into()));
    let (inner_addr, proto) = parse_ws_listen_addr(&addr).unwrap();
    assert_eq!(&inner_addr, &tcp_addr);
    assert_eq!(proto, WsListenProto::Wss("/".into()));

    let mut listen_addr = tcp_addr.clone();
    proto.append_on_addr(&mut listen_addr);
    assert_eq!(listen_addr, addr);

    // Check `/ws`
    let addr = tcp_addr.clone().with(Protocol::Ws("/".into()));
    let (inner_addr, proto) = parse_ws_listen_addr(&addr).unwrap();
    assert_eq!(&inner_addr, &tcp_addr);
    assert_eq!(proto, WsListenProto::Ws("/".into()));

    let mut listen_addr = tcp_addr.clone();
    proto.append_on_addr(&mut listen_addr);
    assert_eq!(listen_addr, addr);
}

#[test]
fn dial_addr() {
    let peer_id = PeerId::random();

    // Check `/tls/ws`
    let addr = "/dns4/example.com/tcp/2222/tls/ws"
        .parse::<Multiaddr>()
        .unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "example.com:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "example.com".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/dns4/example.com/tcp/2222".parse().unwrap());

    // Check `/tls/ws` with `/p2p`
    let addr = format!("/dns4/example.com/tcp/2222/tls/ws/p2p/{peer_id}")
        .parse()
        .unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "example.com:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "example.com".try_into().unwrap());
    assert_eq!(
        info.tcp_addr,
        format!("/dns4/example.com/tcp/2222/p2p/{peer_id}")
            .parse()
            .unwrap()
    );

    // Check `/tls/ws` with `/ip4`
    let addr = "/ip4/127.0.0.1/tcp/2222/tls/ws"
        .parse::<Multiaddr>()
        .unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "127.0.0.1:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "127.0.0.1".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/ip4/127.0.0.1/tcp/2222".parse().unwrap());

    // Check `/tls/ws` with `/ip6`
    let addr = "/ip6/::1/tcp/2222/tls/ws".parse::<Multiaddr>().unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "[::1]:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "::1".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/ip6/::1/tcp/2222".parse().unwrap());

    // Check `/wss`
    let addr = "/dns4/example.com/tcp/2222/wss"
        .parse::<Multiaddr>()
        .unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "example.com:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "example.com".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/dns4/example.com/tcp/2222".parse().unwrap());

    // Check `/wss` with `/p2p`
    let addr = format!("/dns4/example.com/tcp/2222/wss/p2p/{peer_id}")
        .parse()
        .unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "example.com:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "example.com".try_into().unwrap());
    assert_eq!(
        info.tcp_addr,
        format!("/dns4/example.com/tcp/2222/p2p/{peer_id}")
            .parse()
            .unwrap()
    );

    // Check `/wss` with `/ip4`
    let addr = "/ip4/127.0.0.1/tcp/2222/wss".parse::<Multiaddr>().unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "127.0.0.1:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "127.0.0.1".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/ip4/127.0.0.1/tcp/2222".parse().unwrap());

    // Check `/wss` with `/ip6`
    let addr = "/ip6/::1/tcp/2222/wss".parse::<Multiaddr>().unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "[::1]:2222");
    assert_eq!(info.path, "/");
    assert!(info.use_tls);
    assert_eq!(info.server_name, "::1".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/ip6/::1/tcp/2222".parse().unwrap());

    // Check `/ws`
    let addr = "/dns4/example.com/tcp/2222/ws"
        .parse::<Multiaddr>()
        .unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "example.com:2222");
    assert_eq!(info.path, "/");
    assert!(!info.use_tls);
    assert_eq!(info.server_name, "example.com".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/dns4/example.com/tcp/2222".parse().unwrap());

    // Check `/ws` with `/p2p`
    let addr = format!("/dns4/example.com/tcp/2222/ws/p2p/{peer_id}")
        .parse()
        .unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "example.com:2222");
    assert_eq!(info.path, "/");
    assert!(!info.use_tls);
    assert_eq!(info.server_name, "example.com".try_into().unwrap());
    assert_eq!(
        info.tcp_addr,
        format!("/dns4/example.com/tcp/2222/p2p/{peer_id}")
            .parse()
            .unwrap()
    );

    // Check `/ws` with `/ip4`
    let addr = "/ip4/127.0.0.1/tcp/2222/ws".parse::<Multiaddr>().unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "127.0.0.1:2222");
    assert_eq!(info.path, "/");
    assert!(!info.use_tls);
    assert_eq!(info.server_name, "127.0.0.1".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/ip4/127.0.0.1/tcp/2222".parse().unwrap());

    // Check `/ws` with `/ip6`
    let addr = "/ip6/::1/tcp/2222/ws".parse::<Multiaddr>().unwrap();
    let info = parse_ws_dial_addr::<io::Error>(addr).unwrap();
    assert_eq!(info.host_port, "[::1]:2222");
    assert_eq!(info.path, "/");
    assert!(!info.use_tls);
    assert_eq!(info.server_name, "::1".try_into().unwrap());
    assert_eq!(info.tcp_addr, "/ip6/::1/tcp/2222".parse().unwrap());

    // Check non-ws address
    let addr = "/ip4/127.0.0.1/tcp/2222".parse::<Multiaddr>().unwrap();
    parse_ws_dial_addr::<io::Error>(addr).unwrap_err();
}
