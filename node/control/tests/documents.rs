//! 验证白皮书与两张图片引用唯一原件及章节顺序。

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
fn chapter_five_matches_current_module_groups() {
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

#[test]
fn personal_belongs_to_misc_and_entitybase_has_no_independent_state() {
    let source = include_str!("../../../CitizenChainWhitepaper.md");
    let manifest = include_str!("../../../runtime/Cargo.toml");
    // 白皮书归属同时核对真实模块声明，防止个人多签重新进入实体模组。
    for (name, path, heading) in [
        ("entitybase", "entity/entitybase", "5.6.1.实体基础库"),
        ("personal", "misc/personal", "5.9.5.个人多签"),
    ] {
        assert!(manifest.contains(&format!("{name} = {{ path = \"{path}\"")));
        assert!(source.contains(&format!("### {heading}<br>")));
    }
    let entity = source
        .split("## 5.6.实体模组")
        .nth(1)
        .unwrap()
        .split("## 5.7.发行模组")
        .next()
        .unwrap();
    assert!(!entity.contains("personal"));
    assert!(entity.contains("不独立保存链上业务状态，也不提供独立交易入口"));
    let misc = source
        .split("## 5.9.其他模组")
        .nth(1)
        .unwrap()
        .split("# 6.节点程序")
        .next()
        .unwrap();
    assert!(misc.contains("### 5.9.5.个人多签"));
    assert!(misc.contains("不以取得公民 CID 为创建或参与条件"));
    assert!(misc.contains("neither creation nor participation requires a citizen CID"));
}

#[test]
fn retired_modules_and_institution_administrator_titles_do_not_return() {
    let source = include_str!("../../../CitizenChainWhitepaper.md");
    for retired in [
        "管理员",
        "admins",
        "personaladmin",
        "privateadmin",
        "publicadmin",
        "personalentity",
        "administrator",
    ] {
        assert!(!source.contains(retired), "旧名称残留：{retired}");
    }
    for role in [
        "国家储委会委员",
        "省储委会委员",
        "省储行董事",
        "省专员",
        "首席大法官",
        "护宪大法官",
    ] {
        assert!(source.contains(role), "岗位称谓缺失：{role}");
    }
}
