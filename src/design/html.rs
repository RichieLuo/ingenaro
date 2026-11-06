use anyhow::{Context, Result, bail, ensure};

pub const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

/// Structural validation, not a sanitizer. Always use `preview_document` to run
/// generated HTML. The deliberately strict contract rejects markdown/fragments,
/// malformed document boundaries and missing head/body instead of repairing them.
pub fn validate_html(source: &str) -> Result<()> {
    ensure!(
        !source.is_empty() && source.len() <= MAX_HTML_BYTES,
        "HTML 文档不能为空或超过 2 MiB"
    );
    ensure!(!source.contains('\0'), "HTML 文档不能包含空字符");
    let source = source.trim_start_matches('\u{feff}').trim();
    let first_end = source
        .find('>')
        .context("需要完整 HTML 文档，而不是 Markdown 或片段")?;
    let doctype = &source[..=first_end];
    ensure!(
        doctype.starts_with('<') && doctype.len() >= 15,
        "HTML 必须以 <!doctype html> 开始"
    );
    let mut declaration = doctype[1..doctype.len() - 1].split_ascii_whitespace();
    ensure!(
        declaration
            .next()
            .is_some_and(|token| token.eq_ignore_ascii_case("!doctype"))
            && declaration
                .next()
                .is_some_and(|token| token.eq_ignore_ascii_case("html"))
            && declaration.next().is_none(),
        "HTML 必须以 <!doctype html> 开始"
    );

    let mut rest = &source[first_end + 1..];
    // Required sequence: html, head, /head, body, /body, /html. Actual tags
    // only: strings in comments, raw text, and quoted attributes cannot satisfy it.
    let mut stage = 0;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            ensure!(
                matches!(stage, 2 | 4) || rest.trim().is_empty(),
                "HTML 文档边界之外存在内容"
            );
            break;
        };
        ensure!(
            matches!(stage, 2 | 4) || rest[..open].trim().is_empty(),
            "HTML 文档边界之外存在内容"
        );
        rest = &rest[open..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").context("HTML 注释未闭合")?;
            rest = &rest[end + 3..];
            continue;
        }
        let end = tag_end(rest).context("HTML 标签或属性未闭合")?;
        let token = &rest[1..end];
        rest = &rest[end + 1..];
        let token = token.trim();
        ensure!(
            !token.starts_with('!') && !token.starts_with('?'),
            "HTML 只允许一个文档声明"
        );
        let closing = token.starts_with('/');
        let inner = if closing { &token[1..] } else { token };
        let name_end = inner
            .find(|c: char| c.is_ascii_whitespace() || c == '/')
            .unwrap_or(inner.len());
        let name = inner[..name_end].to_ascii_lowercase();
        let suffix = inner[name_end..].trim();
        ensure!(!name.is_empty(), "HTML 标签名称为空");
        if ["html", "head", "body"].contains(&name.as_str()) {
            let expected = [
                ("html", false),
                ("head", false),
                ("head", true),
                ("body", false),
                ("body", true),
                ("html", true),
            ];
            ensure!(
                stage < expected.len() && expected[stage] == (name.as_str(), closing),
                "HTML 需要按顺序包含完整的 html / head / body 标签"
            );
            ensure!(
                !suffix.ends_with('/') && (!closing || suffix.is_empty()),
                "HTML 文档边界标签格式无效"
            );
            stage += 1;
        } else {
            ensure!(matches!(stage, 2 | 4), "HTML 元素必须位于 head 或 body 中");
            ensure!(name != "plaintext", "HTML 不支持 plaintext 元素");
        }
        if !closing
            && [
                "script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes",
            ]
            .contains(&name.as_str())
        {
            rest = skip_raw_text(rest, &name)?;
        }
    }
    ensure!(
        stage == 6,
        "需要完整 HTML 文档，必须包含闭合的 html、head 和 body 标签"
    );
    Ok(())
}

fn tag_end(source: &str) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in source.char_indices().skip(1) {
        match (quote, character) {
            (Some(q), c) if q == c => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, '>') => return Some(offset),
            _ => {}
        }
    }
    None
}

fn skip_raw_text<'a>(source: &'a str, name: &str) -> Result<&'a str> {
    let closing = format!("</{name}");
    let bytes = source.as_bytes();
    for (index, window) in bytes.windows(closing.len()).enumerate() {
        if window.eq_ignore_ascii_case(closing.as_bytes()) {
            let suffix = &source[index + closing.len()..];
            if suffix.starts_with('>') || suffix.starts_with(|c: char| c.is_ascii_whitespace()) {
                let end = suffix.find('>').context("HTML 原始文本结束标签未闭合")?;
                ensure!(suffix[..end].trim().is_empty(), "HTML 原始文本结束标签无效");
                return Ok(&suffix[end + 1..]);
            }
        }
    }
    bail!("HTML 的 {name} 元素未闭合")
}
