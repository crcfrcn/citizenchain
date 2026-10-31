use crate::cid::validate_base_url as endpoint;

#[test]
fn https_base_urls_preserve_the_explicit_origin() {
    for url in [
        "https://onchina.crcfrcn.com:1988",
        "https://127.0.0.1:1988",
        "https://example.com/prefix",
    ] {
        assert_eq!(endpoint(url).unwrap(), url);
    }
    assert_eq!(
        endpoint("https://example.com/").unwrap(),
        "https://example.com"
    );
}

#[test]
fn plaintext_credentials_and_ambiguous_urls_are_rejected() {
    for value in [
        "",
        "http://onchina.invalid",
        "ws://onchina.invalid",
        "wss://localhost:1988",
        "https://user:secret@example.com",
        "https://example.com?x=1",
        "https://example.com#fragment",
        "//example.com",
        "https://",
    ] {
        assert!(endpoint(value).is_err());
    }
}
