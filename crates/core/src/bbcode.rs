//! Reads the markup Workshop descriptions are written in, so that they can
//! be shown with their headings, lists and links.
//!
//! Steam's flavour of BBCode is loose: authors leave tags open, nest them the
//! wrong way round and invent their own. Nothing here fails; what is not
//! understood is shown as it was written.

/// A run of text in one style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Where the text leads. Only ever a web address.
    pub link: Option<String>,
}

impl Span {
    fn same_style(&self, other: &Span) -> bool {
        (self.bold, self.italic, self.underline, self.strike, &self.link)
            == (other.bold, other.italic, other.underline, other.strike, &other.link)
    }
}

/// One line or paragraph of a description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text(Vec<Span>),
    Heading(Vec<Span>),
    /// An entry of a list.
    Item(Vec<Span>),
    Quote(Vec<Span>),
    Code(String),
    /// The address of a picture.
    Image(String),
    Rule,
    /// An empty line between paragraphs.
    Gap,
}

/// Tags are short; a `[` with no `]` this close is not one.
const MAX_TAG: usize = 300;

pub fn parse(source: &str) -> Vec<Block> {
    let source = source.replace('\r', "");
    let mut parser = Parser { line_start: true, ..Default::default() };
    let mut rest = source.as_str();
    while let Some(first) = rest.chars().next() {
        if first == '\n' {
            parser.newline();
            rest = &rest[1..];
            continue;
        }
        if first == '[' {
            if let Some(after) = read_tag(rest).and_then(|(tag, after)| parser.tag(&tag, after)) {
                rest = after;
                continue;
            }
        }
        let end = rest
            .char_indices()
            .skip(1)
            .find(|(_, c)| matches!(c, '[' | '\n'))
            .map_or(rest.len(), |(at, _)| at);
        parser.text(&rest[..end]);
        rest = &rest[end..];
    }
    parser.end_line();
    while parser.blocks.last() == Some(&Block::Gap) {
        parser.blocks.pop();
    }
    parser.blocks
}

struct Tag<'a> {
    /// In small letters.
    name: String,
    value: Option<&'a str>,
    closing: bool,
}

/// The tag `text` starts with, and what follows it.
fn read_tag(text: &str) -> Option<(Tag<'_>, &str)> {
    let inner = text.strip_prefix('[')?;
    let end = inner.char_indices().take_while(|(at, _)| *at < MAX_TAG).find(|(_, c)| *c == ']')?.0;
    let (inner, after) = (&inner[..end], &inner[end + 1..]);
    if inner.contains(['\n', '[']) {
        return None;
    }
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(inner) => (true, inner),
        None => (false, inner),
    };
    let (name, value) = match inner.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (inner, None),
    };
    let name = name.to_ascii_lowercase();
    let known = name == "*" || (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()));
    known.then_some((Tag { name, value, closing }, after))
}

/// What comes before the closing tag `close`, and what comes after it.
fn until<'a>(text: &'a str, close: &str) -> Option<(&'a str, &'a str)> {
    // Small letters take as many bytes as capitals, so the place is the same.
    let at = text.to_ascii_lowercase().find(close)?;
    Some((&text[..at], &text[at + close.len()..]))
}

/// `address` if it is one a browser can be sent to.
fn web_link(address: &str) -> Option<String> {
    let address = address.trim().trim_matches(['"', '\'']);
    let lower = address.to_ascii_lowercase();
    let is_web = lower.starts_with("https://") || lower.starts_with("http://");
    (is_web && !address.contains(char::is_whitespace)).then(|| address.to_string())
}

#[derive(Default)]
struct Parser {
    blocks: Vec<Block>,
    /// The line being read.
    spans: Vec<Span>,
    /// The style in force; its text is not used.
    style: Span,
    heading: bool,
    lists: usize,
    item: bool,
    quotes: usize,
    /// Nothing has been read since the last line break.
    line_start: bool,
}

impl Parser {
    /// Acts on a tag. Returns what is left to read, or `None` for a tag that
    /// is not understood and should be shown as text.
    fn tag<'a>(&mut self, tag: &Tag, after: &'a str) -> Option<&'a str> {
        let opening = !tag.closing;
        match tag.name.as_str() {
            "b" => self.style.bold = opening,
            "i" => self.style.italic = opening,
            "u" => self.style.underline = opening,
            "s" | "strike" => self.style.strike = opening,
            "spoiler" => {}
            "h1" | "h2" | "h3" => {
                self.end_block();
                self.heading = opening;
            }
            "list" | "olist" => {
                self.end_block();
                self.item = false;
                self.lists = if opening { self.lists + 1 } else { self.lists.saturating_sub(1) };
            }
            "*" if opening => {
                self.end_block();
                self.item = true;
            }
            "quote" => {
                self.end_block();
                self.quotes = if opening { self.quotes + 1 } else { self.quotes.saturating_sub(1) };
            }
            "hr" => {
                self.end_block();
                if opening {
                    self.blocks.push(Block::Rule);
                }
            }
            "table" | "tr" => self.end_block(),
            "td" | "th" => {
                if tag.closing {
                    self.push("   ", None);
                }
            }
            "url" if tag.closing => self.style.link = None,
            "url" => match tag.value {
                Some(address) => self.style.link = web_link(address),
                None => {
                    let (address, after) = until(after, "[/url]")?;
                    self.push(address, Some(web_link(address)));
                    return Some(after);
                }
            },
            "code" if opening => {
                let (code, after) = until(after, "[/code]").unwrap_or((after, ""));
                self.end_block();
                self.blocks.push(Block::Code(code.trim_matches('\n').to_string()));
                return Some(after);
            }
            "noparse" if opening => {
                let (text, after) = until(after, "[/noparse]")?;
                for (i, line) in text.split('\n').enumerate() {
                    if i > 0 {
                        self.newline();
                    }
                    self.push(line, None);
                }
                return Some(after);
            }
            "img" if opening => {
                let (address, after) = until(after, "[/img]")?;
                self.end_block();
                if let Some(address) = web_link(address) {
                    self.blocks.push(Block::Image(address));
                }
                return Some(after);
            }
            "previewyoutube" if opening => {
                let video = tag.value.and_then(|value| value.split(';').next()).unwrap_or_default();
                let after = until(after, "[/previewyoutube]").map_or(after, |(_, after)| after);
                self.end_block();
                if !video.is_empty() && video.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c)) {
                    let link = Some(Some(format!("https://www.youtube.com/watch?v={video}")));
                    self.push("Video on YouTube", link);
                    self.end_block();
                }
                return Some(after);
            }
            _ => return None,
        }
        Some(after)
    }

    /// Adds text in the style in force, making web addresses in it links.
    fn text(&mut self, text: &str) {
        if self.style.link.is_some() {
            return self.push(text, None);
        }
        let mut rest = text;
        while let Some(at) = [rest.find("https://"), rest.find("http://")].into_iter().flatten().min() {
            let address = &rest[at..];
            let end = address.find(char::is_whitespace).unwrap_or(address.len());
            let address = address[..end].trim_end_matches(['.', ',', ')', '!', '?', ':', ';', '"', '\'']);
            self.push(&rest[..at], None);
            self.push(address, Some(web_link(address)));
            rest = &rest[at + address.len()..];
        }
        self.push(rest, None);
    }

    /// Adds text; `link` replaces the link in force for this text alone.
    fn push(&mut self, text: &str, link: Option<Option<String>>) {
        if text.is_empty() {
            return;
        }
        self.line_start = false;
        let span = Span {
            text: text.to_string(),
            link: link.unwrap_or_else(|| self.style.link.clone()),
            ..self.style.clone()
        };
        match self.spans.last_mut().filter(|last| last.same_style(&span)) {
            Some(last) => last.text.push_str(&span.text),
            None => self.spans.push(span),
        }
    }

    /// A line break in the source: ends the line, and on an empty one
    /// leaves a gap.
    fn newline(&mut self) {
        let blank = self.line_start && !self.heading;
        if !self.end_line() && blank && !matches!(self.blocks.last(), None | Some(Block::Gap)) {
            self.blocks.push(Block::Gap);
        }
        self.line_start = true;
    }

    /// Ends the line for a tag that stands on its own. The line break that
    /// usually follows such a tag is then not an empty line.
    fn end_block(&mut self) {
        self.end_line();
        self.line_start = false;
    }

    /// Turns what has been read of the line into a block. False if there
    /// was nothing.
    fn end_line(&mut self) -> bool {
        let mut spans = std::mem::take(&mut self.spans);
        if let Some(first) = spans.first_mut() {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = spans.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
        spans.retain(|span| !span.text.is_empty());
        if spans.is_empty() {
            return false;
        }
        self.blocks.push(if self.heading {
            Block::Heading(spans)
        } else if std::mem::take(&mut self.item) && self.lists > 0 {
            Block::Item(spans)
        } else if self.quotes > 0 {
            Block::Quote(spans)
        } else {
            Block::Text(spans)
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str) -> Span {
        Span { text: text.to_string(), ..Default::default() }
    }

    fn link(text: &str, address: &str) -> Span {
        Span { link: Some(address.to_string()), ..span(text) }
    }

    #[test]
    fn plain_text_keeps_its_paragraphs() {
        assert_eq!(
            parse("One line.\r\nThe next.\n\n\n\nA new paragraph.\n\n"),
            vec![
                Block::Text(vec![span("One line.")]),
                Block::Text(vec![span("The next.")]),
                Block::Gap,
                Block::Text(vec![span("A new paragraph.")]),
            ]
        );
        assert_eq!(parse(""), vec![]);
        assert_eq!(parse("\n \n"), vec![]);
    }

    #[test]
    fn styles_and_links() {
        let blocks = parse("Plain [b]bold [i]both[/i][/b] [u]under[/u] [strike]gone[/strike]");
        assert_eq!(
            blocks,
            vec![Block::Text(vec![
                span("Plain "),
                Span { bold: true, ..span("bold ") },
                Span { bold: true, italic: true, ..span("both") },
                span(" "),
                Span { underline: true, ..span("under") },
                span(" "),
                Span { strike: true, ..span("gone") },
            ])]
        );
        assert_eq!(
            parse("Use [url=https://example.com/a]this[/url] or [url]http://example.com[/url]."),
            vec![Block::Text(vec![
                span("Use "),
                link("this", "https://example.com/a"),
                span(" or "),
                link("http://example.com", "http://example.com"),
                span("."),
            ])]
        );
    }

    #[test]
    fn bare_addresses_become_links() {
        assert_eq!(
            parse("Discord: https://discord.gg/abc. Site (https://wiremod.com)"),
            vec![Block::Text(vec![
                span("Discord: "),
                link("https://discord.gg/abc", "https://discord.gg/abc"),
                span(". Site ("),
                link("https://wiremod.com", "https://wiremod.com"),
                span(")"),
            ])]
        );
    }

    #[test]
    fn only_web_addresses_are_followed() {
        for address in ["javascript:alert(1)", "file:///C:/Windows", "steam://run/4000", "example.com"] {
            let blocks = parse(&format!("[url={address}]click[/url]"));
            assert_eq!(blocks, vec![Block::Text(vec![span("click")])], "{address}");
        }
        assert_eq!(parse("[img]file:///secret.png[/img]"), vec![]);
    }

    #[test]
    fn headings_lists_quotes_and_rules() {
        let source = "[h1]Features[/h1]\n[list]\n[*]One\n[*][b]Two[/b]\n[/list]\n[hr][/hr]\n\
                      [quote=someone]Said[/quote]\nAfter";
        assert_eq!(
            parse(source),
            vec![
                Block::Heading(vec![span("Features")]),
                Block::Item(vec![span("One")]),
                Block::Item(vec![Span { bold: true, ..span("Two") }]),
                Block::Rule,
                Block::Quote(vec![span("Said")]),
                Block::Text(vec![span("After")]),
            ]
        );
    }

    #[test]
    fn code_pictures_and_videos() {
        let source = "[code]\nlua_run [b]x[/b]\n[/code]\n[IMG]https://example.com/a.png[/IMG]\n\
                      [previewyoutube=iqHN5UKtA2w;full][/previewyoutube]";
        assert_eq!(
            parse(source),
            vec![
                Block::Code("lua_run [b]x[/b]".to_string()),
                Block::Image("https://example.com/a.png".to_string()),
                Block::Text(vec![link("Video on YouTube", "https://www.youtube.com/watch?v=iqHN5UKtA2w")]),
            ]
        );
        assert_eq!(
            parse("[noparse][b]as written[/b][/noparse]"),
            vec![Block::Text(vec![span("[b]as written[/b]")])]
        );
    }

    #[test]
    fn what_is_not_understood_is_shown_as_written() {
        assert_eq!(parse("[WIP] my [addon]"), vec![Block::Text(vec![span("[WIP] my [addon]")])]);
        assert_eq!(
            parse("a [ b ] c [url]unclosed"),
            vec![Block::Text(vec![span("a [ b ] c [url]unclosed")])]
        );
        assert_eq!(
            parse("[b]never closed"),
            vec![Block::Text(vec![Span { bold: true, ..span("never closed") }])]
        );
        assert_eq!(parse("[/list][/quote]fine"), vec![Block::Text(vec![span("fine")])]);
        assert_eq!(
            parse("Ünïcödé [b]☃[/b]"),
            vec![Block::Text(vec![span("Ünïcödé "), Span { bold: true, ..span("☃") },])]
        );
    }
}
