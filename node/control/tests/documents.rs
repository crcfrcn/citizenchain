use crate::documents::{get_other_tabs_content, get_whitepaper_document};

#[test]
fn bundled_whitepaper_uses_root_source_and_embeds_both_original_images() {
    // 验证命令实际返回完整正文及可解码的原始配图，无须访问网络。
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let document = runtime.block_on(get_whitepaper_document()).unwrap();
    let mut expected = include_str!("../../../CitizenChainWhitepaper.md").to_string();
    for (name, svg) in [
        (
            "whitepaper-node-diagram.svg",
            include_str!("../../../icons/whitepaper-node-diagram.svg"),
        ),
        (
            "whitepaper-reserve-architecture.svg",
            include_str!("../../../icons/whitepaper-reserve-architecture.svg"),
        ),
    ] {
        let reference = format!("./icons/{name}");
        assert!(expected.contains(&reference));
        // 按原文位置逐张解码，防止只内嵌其中一张。
        let prefix = expected.split(&reference).next().unwrap();
        let actual = &document.markdown[prefix.len()..];
        assert!(actual.starts_with("data:image/svg+xml,"));
        let encoded = actual["data:image/svg+xml,".len()..]
            .split('"')
            .next()
            .unwrap();
        let decoded: Vec<u8> = encoded
            .as_bytes()
            .chunks_exact(3)
            .map(|part| {
                assert_eq!(part[0], b'%');
                u8::from_str_radix(std::str::from_utf8(&part[1..]).unwrap(), 16).unwrap()
            })
            .collect();
        assert_eq!(decoded, svg.as_bytes());
        expected = expected.replacen(&reference, &format!("data:image/svg+xml,{encoded}"), 1);
    }
    assert_eq!(document.markdown, expected);
    assert!(!document.markdown.contains("./icons/whitepaper-"));
    assert!(get_other_tabs_content()
        .unwrap()
        .tabs
        .iter()
        .all(|tab| tab.key != "whitepaper"));
}

#[test]
fn chapter_five_places_primitives_after_genesis_and_renumbers_following_modules() {
    let source = include_str!("../../../CitizenChainWhitepaper.md");
    let headings: Vec<_> = source
        .lines()
        .filter(|line| line.starts_with("## 5."))
        .collect();
    let names = [
        "genesis",
        "primitives",
        "votingengine",
        "governance",
        "admins",
        "public",
        "entity",
        "issuance",
        "transaction",
        "misc",
    ];
    assert_eq!(headings.len(), names.len());
    for (index, (heading, name)) in headings.iter().zip(names).enumerate() {
        assert!(heading.starts_with(&format!("## 5.{}.", index + 1)));
        assert!(heading.contains(&format!(">5.{}. {name}</span>", index + 1)));
    }
}
