//! 链上结构化宪法的桌面展示能力。
//!
//! 本文件只负责解码展示字段和生成 HTML，不参与区块导入判定。最高规则的执法逻辑固定留在
//! `guard.rs`，避免 UI 渲染调整误触共识守卫。

use super::*;

/// 完整 HTML 外壳模板，内含版本、目录和正文占位标记。
const SHELL: &str = include_str!("constitution_shell.html");
const VERSION_LABEL_MARKER: &str = "<!--CONSTITUTION_VERSION_LABEL-->";
const TOC_MARKER: &str = "<!--CONSTITUTION_TOC-->";
const CONTENT_MARKER: &str = "<!--CONSTITUTION_CONTENT-->";

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn opt_text(bytes: &Option<Vec<u8>>) -> String {
    bytes.as_ref().map(|b| text(b)).unwrap_or_default()
}

/// HTML 文本转义；链上立法文本仍不得直接破坏桌面端文档外壳。
fn esc(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// 当前生效版本号，桌面端不得提前展示待生效版本。
pub fn effective_version_of_law(law_scale: &[u8]) -> Result<u32, String> {
    let law = decode_law_head(law_scale).map_err(|e| format!("宪法 Law 解码失败:{e:?}"))?;
    law.effective_version
        .ok_or_else(|| "宪法尚无生效版本".to_string())
}

/// 解码链上不可修改条款 manifest，供桌面端展示徽章。
pub fn immutable_article_numbers(manifest_scale: &[u8]) -> Result<Vec<u32>, String> {
    let manifest = MImmutableManifest::decode(&mut &manifest_scale[..])
        .map_err(|e| format!("宪法不可修改条款 manifest 解码失败:{e}"))?;
    Ok(manifest.article_numbers)
}

/// 把链上结构化宪法 SCALE 字节重建为完整 HTML 文档。
pub fn render_constitution_html(
    law_version_scale: &[u8],
    immutable_article_numbers: &[u32],
    law_version_label_scale: Option<&[u8]>,
) -> Result<String, String> {
    let law = MLawVersionHead::decode(&mut &law_version_scale[..])
        .map_err(|e| format!("宪法 LawVersion 解码失败:{e}"))?;
    let version_label = law_version_label_scale
        .map(|raw| {
            MLawVersionLabel::decode(&mut &raw[..])
                .map_err(|e| format!("宪法 LawVersionLabel 解码失败:{e}"))
        })
        .transpose()?;
    let version_label_html = render_version_label(law.version, version_label.as_ref());

    let mut toc = String::new();
    let mut content = String::new();

    for chapter in &law.chapters {
        let (c_cn, c_en) = (
            esc(&text(&chapter.title)),
            esc(&opt_text(&chapter.title_en)),
        );
        toc.push_str(&format!(
            "        <a class=\"toc-item toc-level-1\" href=\"#chapter-{n}\"><span class=\"toc-cn\">{c_cn}</span><span class=\"toc-en\">{c_en}</span></a>\n",
            n = chapter.number,
        ));
        content.push_str(&format!(
            "<section id=\"chapter-{n}\" class=\"block chapter-block\">\n  <h1 class=\"chapter-title\">\n    <span class=\"cn heading-cn\">{c_cn}</span>\n    <span class=\"en heading-en\">{c_en}</span>\n  </h1>\n</section>\n\n",
            n = chapter.number,
        ));

        for section in &chapter.sections {
            let (s_cn, s_en) = (
                esc(&text(&section.title)),
                esc(&opt_text(&section.title_en)),
            );
            toc.push_str(&format!(
                "        <a class=\"toc-item toc-level-2\" href=\"#chapter-{cn}-section-{sn}\"><span class=\"toc-cn\">{s_cn}</span><span class=\"toc-en\">{s_en}</span></a>\n",
                cn = chapter.number, sn = section.number,
            ));
            content.push_str(&format!(
                "<section id=\"chapter-{cn}-section-{sn}\" class=\"block section-block\">\n  <h2 class=\"section-title\">\n    <span class=\"cn heading-cn\">{s_cn}</span>\n    <span class=\"en heading-en\">{s_en}</span>\n  </h2>\n</section>\n\n",
                cn = chapter.number, sn = section.number,
            ));

            for article in &section.articles {
                let (a_cn, a_en) = (
                    esc(&text(&article.title)),
                    esc(&opt_text(&article.title_en)),
                );
                let (immutable_badge_cn, immutable_badge_en) = if immutable_article_numbers
                    .contains(&article.number)
                {
                    (
                            "<span class=\"immutable-badge immutable-badge-cn\">不可修改条款</span>",
                            "<span class=\"immutable-badge immutable-badge-en\">Immutable Clause</span>",
                        )
                } else {
                    ("", "")
                };
                toc.push_str(&format!(
                    "        <a class=\"toc-item toc-level-3\" href=\"#article-{an}\"><span class=\"toc-cn\">{a_cn}</span><span class=\"toc-en\">{a_en}</span></a>\n",
                    an = article.number,
                ));

                let mut paragraphs = format!(
                    "  <p class=\"article-paragraph\">\n    <span class=\"cn body-cn\">{b_cn}</span>\n    <span class=\"en body-en\">{b_en}</span>\n  </p>\n",
                    b_cn = esc(&text(&article.body)),
                    b_en = esc(&opt_text(&article.body_en)),
                );
                for clause in &article.clauses {
                    paragraphs.push_str(&format!(
                        "  <p class=\"article-paragraph\">\n    <span class=\"cn body-cn\">{k_cn}</span>\n    <span class=\"en body-en\">{k_en}</span>\n  </p>\n",
                        k_cn = esc(&text(&clause.text)),
                        k_en = esc(&opt_text(&clause.text_en)),
                    ));
                }

                content.push_str(&format!(
                    "<article id=\"article-{an}\" class=\"block article-block\">\n  <h3 class=\"article-title\">\n    <span class=\"cn heading-cn\">{a_cn}{immutable_badge_cn}</span>\n    <span class=\"en heading-en\">{a_en}{immutable_badge_en}</span>\n  </h3>\n\n{paragraphs}</article>\n\n",
                    an = article.number,
                ));
            }
        }
    }

    Ok(SHELL
        .replace(VERSION_LABEL_MARKER, &version_label_html)
        .replace(TOC_MARKER, &toc)
        .replace(CONTENT_MARKER, &content))
}

fn render_version_label(version: u32, label: Option<&MLawVersionLabel>) -> String {
    let fallback = format!("v{version}");
    let (cn, en) = if let Some(label) = label {
        let cn = text(&label.title);
        let en = opt_text(&label.title_en);
        (
            if cn.trim().is_empty() {
                fallback.clone()
            } else {
                cn
            },
            if en.trim().is_empty() {
                fallback.clone()
            } else {
                en
            },
        )
    } else {
        (fallback.clone(), fallback)
    };
    format!(
        "<span class=\"doc-version-cn\">{}</span><span class=\"doc-version-en\">{}</span>",
        esc(&cn),
        esc(&en),
    )
}
