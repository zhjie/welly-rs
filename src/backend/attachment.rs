#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageAttachment {
    pub filename: String,
    pub size: String,
    pub image_url: String,
    pub article_url: Option<String>,
}

pub fn parse_image_attachments(_screen_text: &str) -> Vec<ImageAttachment> {
    let lines: Vec<&str> = _screen_text.lines().map(str::trim).collect();
    let mut attachments = Vec::new();
    let mut article_url = None;
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];

        if let Some((filename, size)) = parse_attachment_header(line) {
            if let Some((url_index, image_url)) = next_non_empty_url(&lines, index + 1) {
                attachments.push(ImageAttachment {
                    filename,
                    size,
                    image_url: image_url.to_owned(),
                    article_url: None,
                });
                index = url_index;
            }
        } else if let Some(url) = parse_article_url(line) {
            article_url = Some(url.to_owned());
        }

        index += 1;
    }

    if let Some(article_url) = article_url {
        for attachment in &mut attachments {
            attachment.article_url = Some(article_url.clone());
        }
    }

    attachments
}

fn parse_attachment_header(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("附图:")?.trim();
    let rest = rest.strip_suffix("链接:")?.trim();
    let open_paren = rest.rfind('(')?;
    let close_paren = rest.rfind(')')?;
    if close_paren <= open_paren {
        return None;
    }

    let filename = rest[..open_paren].trim();
    let size = rest[(open_paren + 1)..close_paren].trim();
    if filename.is_empty() || size.is_empty() {
        return None;
    }

    Some((filename.to_owned(), size.to_owned()))
}

fn next_non_empty_url<'a>(lines: &'a [&str], start: usize) -> Option<(usize, &'a str)> {
    lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, line)| {
            if line.is_empty() {
                None
            } else if is_http_url(line) {
                Some((index, *line))
            } else {
                None
            }
        })
}

fn parse_article_url(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("全文：")
        .or_else(|| line.strip_prefix("全文:"))?
        .trim();
    is_http_url(rest).then_some(rest)
}

fn is_http_url(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::{parse_image_attachments, ImageAttachment};

    #[test]
    fn parses_newsmth_image_attachment_footer() {
        let screen = r#"发信人: kingstarxin (凌峰), 信区: NewExpress
标  题: Re: dy 旅客都说西安的羊肉泡馍太贵
发信站: 水木社区 (Sat May  9 00:15:12 2026), 站内

二两羊肉怎么不也得十五元，加十元的泡馍，里面有两个馍，正常够两个南方人的饭量的，
25不贵吧

附图: 96a49d81065fbe27660d1f8d18b551b7.png (568 KB) 链接:
https://www.newsmth.net/att.php?953K4O6E0HiP/aUQFD3bEAgAALcIAAHkBAAAuFy9r.png
全文：https://www.newsmth.net/nForum/article/NewExpress/46429609?s=46429711
"#;

        assert_eq!(
            parse_image_attachments(screen),
            vec![ImageAttachment {
                filename: "96a49d81065fbe27660d1f8d18b551b7.png".to_owned(),
                size: "568 KB".to_owned(),
                image_url:
                    "https://www.newsmth.net/att.php?953K4O6E0HiP/aUQFD3bEAgAALcIAAHkBAAAuFy9r.png"
                        .to_owned(),
                article_url: Some(
                    "https://www.newsmth.net/nForum/article/NewExpress/46429609?s=46429711"
                        .to_owned()
                ),
            }]
        );
    }

    #[test]
    fn parses_multiple_newsmth_image_attachment_footers() {
        let screen = r#"正文

附图: first.png (12 KB) 链接:
https://www.newsmth.net/att.php?first.png
附图: second.jpg (3 MB) 链接:
https://www.newsmth.net/att.php?second.jpg
全文：https://www.newsmth.net/nForum/article/NewExpress/1
"#;

        let attachments = parse_image_attachments(screen);

        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].filename, "first.png");
        assert_eq!(
            attachments[0].image_url,
            "https://www.newsmth.net/att.php?first.png"
        );
        assert_eq!(attachments[1].filename, "second.jpg");
        assert_eq!(attachments[1].size, "3 MB");
        assert_eq!(
            attachments[1].article_url.as_deref(),
            Some("https://www.newsmth.net/nForum/article/NewExpress/1")
        );
    }
}
