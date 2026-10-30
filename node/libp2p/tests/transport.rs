use super::{tls, WsConfig};
use futures::prelude::*;
use libp2p_core::{
    multiaddr::Protocol,
    transport::{DialOpts, ListenerId, PortUse},
    Endpoint, Multiaddr, Transport,
};
use libp2p_identity::PeerId;
use libp2p_tcp as tcp;

#[test]
fn dialer_connects_to_listener_ipv4() {
    let a = "/ip4/127.0.0.1/tcp/0/ws".parse().unwrap();
    futures::executor::block_on(connect(a))
}

#[test]
fn dialer_connects_to_listener_ipv6() {
    let a = "/ip6/::1/tcp/0/ws".parse().unwrap();
    futures::executor::block_on(connect(a))
}

#[test]
fn dialer_connects_to_listener_wss_ipv4() {
    let certified_key = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let private_key = tls::PrivateKey::new(certified_key.key_pair.serialize_der());
    let certificate = tls::Certificate::new(certified_key.cert.der().to_vec());
    let mut builder = tls::Config::builder();
    builder
        .add_trust(&certificate)
        .expect("trust test certificate")
        .server(private_key, [certificate])
        .expect("configure test TLS server");

    // 真实监听和拨号必须完成 TCP、TLS 与 WebSocket 三层握手。
    let address = "/ip4/127.0.0.1/tcp/0/wss".parse().unwrap();
    futures::executor::block_on(connect_with_tls(address, builder.finish()))
}

fn new_ws_config() -> WsConfig<tcp::async_io::Transport> {
    WsConfig::new(tcp::async_io::Transport::new(tcp::Config::default()))
}

async fn connect(listen_addr: Multiaddr) {
    let mut ws_config = new_ws_config().boxed();
    ws_config
        .listen_on(ListenerId::next(), listen_addr)
        .expect("listener");

    let addr = ws_config
        .next()
        .await
        .expect("no error")
        .into_new_address()
        .expect("listen address");

    assert_eq!(Some(Protocol::Ws("/".into())), addr.iter().nth(2));
    assert_ne!(Some(Protocol::Tcp(0)), addr.iter().nth(1));

    let inbound = async move {
        let (conn, _addr) = ws_config
            .select_next_some()
            .map(|ev| ev.into_incoming())
            .await
            .unwrap();
        conn.await
    };

    let outbound = new_ws_config()
        .boxed()
        .dial(
            addr.with(Protocol::P2p(PeerId::random())),
            DialOpts {
                role: Endpoint::Dialer,
                port_use: PortUse::New,
            },
        )
        .unwrap();

    let (a, b) = futures::join!(inbound, outbound);
    a.and(b).unwrap();
}

async fn connect_with_tls(listen_addr: Multiaddr, tls_config: tls::Config) {
    let mut listener = new_ws_config();
    listener.set_tls_config(tls_config.clone());
    let mut listener = listener.boxed();
    listener
        .listen_on(ListenerId::next(), listen_addr)
        .expect("WSS listener");

    let address = listener
        .next()
        .await
        .expect("WSS listen event")
        .into_new_address()
        .expect("WSS listen address");
    assert_eq!(Some(Protocol::Wss("/".into())), address.iter().nth(2));

    let inbound = async move {
        let (connection, _address) = listener
            .select_next_some()
            .map(|event| event.into_incoming())
            .await
            .expect("WSS incoming connection");
        connection.await
    };

    let mut dialer = new_ws_config();
    dialer.set_tls_config(tls_config);
    let outbound = dialer
        .boxed()
        .dial(
            address.with(Protocol::P2p(PeerId::random())),
            DialOpts {
                role: Endpoint::Dialer,
                port_use: PortUse::New,
            },
        )
        .expect("WSS dial");

    let (inbound, outbound) = futures::join!(inbound, outbound);
    inbound.and(outbound).expect("complete WSS handshake");
}
