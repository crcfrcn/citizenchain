//! 以发布版上游金标验证UR实现，并覆盖业务原文、漏帧、边界、会话隔离和缓冲区重试。

use qr_protocol::transport::{Decoder, Encoder, FRAGMENT_BYTES, INTERVAL_MS, FOREGROUND, MAX_MESSAGE_BYTES};

#[test]
fn upstream_vector_is_not_generated_by_the_implementation_under_test() {
    let value: serde_json::Value = serde_json::from_str(include_str!("transport.json")).unwrap();
    let message = value["message"].as_str().unwrap().repeat(value["repeat"].as_u64().unwrap() as usize);
    let mut encoder = ur::Encoder::new(message.as_bytes(), value["fragment_bytes"].as_u64().unwrap() as usize, value["type"].as_str().unwrap()).unwrap();
    assert_eq!(encoder.next_part().unwrap(), value["first"].as_str().unwrap());
}

#[test]
fn static_sr_sized_message_retains_exact_bytes_and_configuration() {
    assert_eq!((FRAGMENT_BYTES, INTERVAL_MS, FOREGROUND), (400, 800, "#007A74"));
    let text = include_str!("sign_response.json").trim();
    let mut encoder = Encoder::new(text).unwrap();
    assert!(!encoder.animated());
    let first = encoder.next_frame().unwrap();
    assert_eq!(first, encoder.next_frame().unwrap());
    assert!(first.starts_with("UR:CITIZEN/"));
    assert_eq!(Decoder::default().receive(&first).unwrap().text.as_deref(), Some(text));
}

#[test]
fn pq_sized_message_recovers_without_the_original_frames() {
    let text = "公民签名消息AbCd0123".repeat(500);
    let mut encoder = Encoder::new(&text).unwrap();
    assert!(encoder.animated());
    // 完整跳过原始帧；只收后续喷泉冗余帧，验证不是固定帧循环。
    for _ in 0..200 { encoder.next_frame().unwrap(); }
    let mut decoder = Decoder::default();
    let mut result = None;
    for index in 0..2000 {
        let frame = encoder.next_frame().unwrap();
        if index % 3 == 0 { continue; }
        let received = decoder.receive(&frame).unwrap();
        if received.text.is_some() { result = received.text; break; }
        decoder.receive(&frame).unwrap();
    }
    assert_eq!(result.as_deref(), Some(text.as_str()));
}

#[test]
fn invalid_and_mixed_frames_do_not_replace_a_session() {
    let mut a = Encoder::new(&"a".repeat(2000)).unwrap();
    let mut b = Encoder::new(&"b".repeat(2000)).unwrap();
    let mut decoder = Decoder::default();
    assert!(decoder.receive(&a.next_frame().unwrap()).unwrap().text.is_none());
    assert!(decoder.receive(&b.next_frame().unwrap()).is_err());
    assert!(decoder.receive("GMBQR1:invalid").is_err());
    assert!(decoder.receive("UR:BYTES/INVALID").is_err());
    assert!(decoder.receive(&"x".repeat(2049)).is_err());
    assert!(Encoder::new("").is_err());
    assert!(Encoder::new(&"x".repeat(MAX_MESSAGE_BYTES + 1)).is_err());
    let mut complete = None;
    for _ in 0..30 { if let Some(text) = decoder.receive(&a.next_frame().unwrap()).unwrap().text { complete = Some(text); break; } }
    assert_eq!(complete, Some("a".repeat(2000)));
}

#[test]
fn output_retry_does_not_advance_and_sessions_close_independently() {
    let a = qr_protocol::wasm::create(1, b"public message").unwrap();
    let b = qr_protocol::wasm::create(2, b"").unwrap();
    let size = qr_protocol::wasm::step(a, b"", 0).unwrap_err();
    let frame = qr_protocol::wasm::step(a, b"", size).unwrap();
    let result = qr_protocol::wasm::step(b, &frame[2..], 2048).unwrap();
    assert_eq!(&result[..2], &[2, 100]);
    assert_eq!(&result[2..], b"public message");
    qr_protocol::wasm::release(a);
    assert_eq!(qr_protocol::wasm::step(a, b"", 2048).unwrap()[0], 255);
    assert_eq!(qr_protocol::wasm::step(b, b"", 2048).unwrap()[0], 2);
    qr_protocol::wasm::release(b);
}

#[test]
fn complete_message_crc_is_checked_even_when_each_part_checksum_is_valid() {
    let mut encoder=Encoder::new(&"a".repeat(2000)).unwrap();
    let mut receiver=Decoder::default();
    let frame=encoder.next_frame().unwrap().to_ascii_lowercase();
    let (_,mut bytes)=ur::decode(&frame).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    let prefix=frame.rsplit_once('/').unwrap().0;
    let corrupt=format!("{prefix}/{}",ur::bytewords::encode(&bytes,ur::bytewords::Style::Minimal));
    assert!(receiver.receive(&corrupt).unwrap().text.is_none());
    let mut rejected=false;
    for _ in 0..20 {
        match receiver.receive(&encoder.next_frame().unwrap()) {
            Err(error)=>{assert!(error.contains("完整性"));rejected=true;break},
            Ok(value)=>assert!(value.text.is_none()),
        }
    }
    assert!(rejected);
}

#[test]
fn static_density_threshold_is_independent_of_fragment_limit() {
    let mut encoder=Encoder::new(&"x".repeat(400)).unwrap();
    assert!(!encoder.animated());
    let frame=encoder.next_frame().unwrap();
    assert_eq!(Decoder::default().receive(&frame).unwrap().text,Some("x".repeat(400)));
    let profile:serde_json::Value=serde_json::from_str(&qr_protocol::transport::profile()).unwrap();
    assert_eq!(profile["fragment_bytes"],400);assert_eq!(profile["interval_ms"],800);
}

#[test]
fn inactivity_resets_a_session_but_continuous_valid_frames_keep_it_alive() {
    use qr_protocol::transport::IDLE_MS;
    let mut a=Encoder::new(&"a".repeat(2000)).unwrap();let first=a.next_frame().unwrap();
    let mut decoder=Decoder::default();decoder.receive_at(&first,0).unwrap();
    decoder.receive_at(&first,IDLE_MS-1).unwrap();
    let mut b=Encoder::new(&"b".repeat(2000)).unwrap();
    assert!(decoder.receive_at(&b.next_frame().unwrap(),IDLE_MS+1).is_err());
    let mut recovered=None;
    for n in 0..20 {
        let value=decoder.receive_at(&b.next_frame().unwrap(),IDLE_MS*2+n).unwrap();
        if value.text.is_some(){recovered=value.text;break}
    }
    assert_eq!(recovered,Some("b".repeat(2000)));
}
