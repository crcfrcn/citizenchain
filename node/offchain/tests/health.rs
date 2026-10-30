use crate::health::finalize;
use crate::health::run_dns_check;
use crate::types::ConnectivityCheck;

#[test]
fn empty_domain_dns_check_fails_fast() {
    let c = run_dns_check("");
    assert!(!c.ok);
    assert_eq!(c.label, "DNS 解析");
}

#[test]
fn finalize_marks_all_ok_when_every_check_passes() {
    let checks = vec![
        ConnectivityCheck {
            label: "a",
            ok: true,
            detail: None,
        },
        ConnectivityCheck {
            label: "b",
            ok: true,
            detail: None,
        },
    ];
    let r = finalize(checks);
    assert!(r.all_ok);
}

#[test]
fn finalize_flags_failure_when_any_fails() {
    let checks = vec![
        ConnectivityCheck {
            label: "a",
            ok: true,
            detail: None,
        },
        ConnectivityCheck {
            label: "b",
            ok: false,
            detail: Some("x".into()),
        },
    ];
    let r = finalize(checks);
    assert!(!r.all_ok);
}
