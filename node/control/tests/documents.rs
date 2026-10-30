use crate::documents::decode_whitepaper_document;
use crate::documents::get_other_tabs_content;
use crate::documents::WHITEPAPER_CONTENT_LIMIT;

#[test]
fn static_document_accepts_markdown_and_rejects_html_empty_or_oversize() {
    let document = decode_whitepaper_document(r##"{"markdown":"# 白皮书"}"##.as_bytes()).unwrap();
    assert_eq!(document.markdown, "# 白皮书");
    assert!(decode_whitepaper_document(b"<html>fallback</html>").is_err());
    assert!(decode_whitepaper_document(br##"{"markdown":" "}"##).is_err());
    assert!(decode_whitepaper_document(&vec![b'a'; WHITEPAPER_CONTENT_LIMIT + 1]).is_err());
    assert!(get_other_tabs_content()
        .unwrap()
        .tabs
        .iter()
        .all(|tab| tab.key != "whitepaper"));
}
