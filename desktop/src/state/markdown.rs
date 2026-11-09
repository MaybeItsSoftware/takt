//! Markdown read into blocks for the notes editor's preview: what Obsidian
//! shows of a note, closely enough to read it by. Each line is its own
//! block, as Obsidian draws a single line break, so nothing is reflowed.
//!
//! Inline text comes out as plain text and the stretches of it marked
//! bold, italic, code and so on, which never overlap: the window turns
//! each into a run of styled text.

use std::ops::Range;

/// What a stretch of inline text is drawn as.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub highlight: bool,
    /// A `[text](url)` or `[[wiki link]]`, or a bare address.
    pub link: bool,
    /// A `#tag`.
    pub tag: bool,
}

impl Marks {
    pub fn plain(&self) -> bool {
        *self == Marks::default()
    }
}

/// A line's text with its marked stretches, in order and apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub spans: Vec<(Range<usize>, Marks)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        text: Inline,
    },
    Line(Inline),
    /// A list item: its depth (by indent), its bullet or number, and
    /// whether it is a checkbox, ticked or not.
    Item {
        depth: usize,
        marker: String,
        task: Option<bool>,
        text: Inline,
    },
    Quote(Inline),
    /// A fenced block, as written.
    Code {
        lang: String,
        text: String,
    },
    /// The `---` properties at the head of an Obsidian note.
    Properties(String),
    Rule,
    Blank,
}

/// `text` as blocks.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut ix = 0;
    // Front matter: only at the very top.
    if lines.first().is_some_and(|l| l.trim_end() == "---")
        && let Some(end) = lines[1..].iter().position(|l| l.trim_end() == "---")
    {
        out.push(Block::Properties(lines[1..=end].join("\n")));
        ix = end + 2;
    }
    while ix < lines.len() {
        let line = lines[ix];
        let trimmed = line.trim_start();
        if let Some(fence) = fence(trimmed) {
            let lang = trimmed[fence.len()..].trim().to_string();
            let mut body = Vec::new();
            ix += 1;
            while ix < lines.len() && !lines[ix].trim_start().starts_with(fence) {
                body.push(lines[ix]);
                ix += 1;
            }
            out.push(Block::Code {
                lang,
                text: body.join("\n"),
            });
            ix += 1;
            continue;
        }
        out.push(block(line));
        ix += 1;
    }
    // Runs of blank lines draw as one gap.
    out.dedup_by(|a, b| *a == Block::Blank && *b == Block::Blank);
    out
}

fn fence(line: &str) -> Option<&'static str> {
    if line.starts_with("```") {
        Some("```")
    } else if line.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn block(line: &str) -> Block {
    let trimmed = line.trim_start();
    if trimmed.trim().is_empty() {
        return Block::Blank;
    }
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() >= 3
        && ["-", "*", "_"]
            .iter()
            .any(|c| compact.chars().all(|x| x.to_string() == *c))
    {
        return Block::Rule;
    }
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        return Block::Heading {
            level: hashes as u8,
            text: inline(trimmed[hashes..].trim()),
        };
    }
    if let Some(rest) = trimmed.strip_prefix('>') {
        // Obsidian's callouts (`> [!note]`) read as their title.
        let rest = rest.trim_start();
        let rest = match rest.strip_prefix("[!") {
            Some(callout) => callout
                .split_once(']')
                .map_or(rest, |(_, title)| title.trim()),
            None => rest,
        };
        return Block::Quote(inline(rest));
    }
    let indent = line.len() - trimmed.len();
    let depth = line[..indent]
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum::<usize>()
        / 2;
    let bullet = ["- ", "* ", "+ "]
        .iter()
        .find(|b| trimmed.starts_with(**b))
        .map(|b| ("•".to_string(), &trimmed[b.len()..]));
    let numbered = || {
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        let after = &trimmed[digits..];
        (digits > 0 && (after.starts_with(". ") || after.starts_with(") ")))
            .then(|| (trimmed[..digits + 1].to_string(), &after[2..]))
    };
    if let Some((marker, rest)) = bullet.or_else(numbered) {
        let (task, rest) = match rest.get(..4) {
            Some("[ ] ") => (Some(false), &rest[4..]),
            Some("[x] " | "[X] ") => (Some(true), &rest[4..]),
            _ if rest == "[ ]" => (Some(false), ""),
            _ if rest == "[x]" || rest == "[X]" => (Some(true), ""),
            _ => (None, rest),
        };
        return Block::Item {
            depth,
            marker,
            task,
            text: inline(rest),
        };
    }
    Block::Line(inline(trimmed))
}

/// One line's inline markup read off: what it says and what is marked.
pub fn inline(text: &str) -> Inline {
    let mut out = Inline::default();
    walk(text, Marks::default(), &mut out);
    // Neighbouring stretches marked alike join.
    let mut spans: Vec<(Range<usize>, Marks)> = Vec::new();
    for (range, marks) in out.spans.drain(..) {
        match spans.last_mut() {
            Some((last, m)) if *m == marks && last.end == range.start => last.end = range.end,
            _ => spans.push((range, marks)),
        }
    }
    out.spans = spans;
    out
}

fn push(out: &mut Inline, text: &str, marks: Marks) {
    if text.is_empty() {
        return;
    }
    let start = out.text.len();
    out.text.push_str(text);
    if !marks.plain() {
        out.spans.push((start..out.text.len(), marks));
    }
}

/// The pairs that wrap marked text, longest first so `**` beats `*`.
/// How a delimiter marks what it wraps.
type Mark = fn(&mut Marks);

const PAIRS: [(&str, Mark); 7] = [
    ("**", |m| m.bold = true),
    ("__", |m| m.bold = true),
    ("~~", |m| m.strike = true),
    ("==", |m| m.highlight = true),
    ("*", |m| m.italic = true),
    ("_", |m| m.italic = true),
    ("`", |m| m.code = true),
];

fn walk(text: &str, marks: Marks, out: &mut Inline) {
    let mut plain_from = 0;
    let mut ix = 0;
    while ix < text.len() {
        let rest = &text[ix..];
        if let Some((taken, emit)) = token(rest, ix, text, marks) {
            push(out, &text[plain_from..ix], marks);
            emit(out);
            ix += taken;
            plain_from = ix;
            continue;
        }
        ix += rest.chars().next().map_or(1, char::len_utf8);
    }
    push(out, &text[plain_from..], marks);
}

type Emit<'a> = Box<dyn FnOnce(&mut Inline) + 'a>;

/// A piece of markup starting at `rest` (`at` into `whole`): how many bytes
/// it takes and how to write it out.
fn token<'a>(rest: &'a str, at: usize, whole: &'a str, marks: Marks) -> Option<(usize, Emit<'a>)> {
    if marks.code {
        return None;
    }
    // `code` first: nothing inside it is markup.
    if let Some(inner) = rest.strip_prefix('`')
        && let Some(end) = inner.find('`')
        && end > 0
    {
        let code = Marks {
            code: true,
            ..marks
        };
        return Some((end + 2, Box::new(move |out| push(out, &inner[..end], code))));
    }
    // [[Note]], [[Note|shown]], ![[embed]]
    let wiki = rest.strip_prefix("![[").or_else(|| rest.strip_prefix("[["));
    if let Some(inner) = wiki
        && let Some(end) = inner.find("]]")
    {
        let prefix = rest.len() - inner.len();
        let target = &inner[..end];
        let shown = target.split_once('|').map_or(target, |(_, alias)| alias);
        let link = Marks {
            link: true,
            ..marks
        };
        return Some((
            prefix + end + 2,
            Box::new(move |out| push(out, shown, link)),
        ));
    }
    // [text](url)
    if let Some(inner) = rest.strip_prefix('[')
        && let Some(close) = inner.find("](")
        && let Some(paren) = inner[close + 2..].find(')')
    {
        let label = &inner[..close];
        let link = Marks {
            link: true,
            ..marks
        };
        return Some((
            1 + close + 2 + paren + 1,
            Box::new(move |out| walk(label, link, out)),
        ));
    }
    // A bare address.
    if (rest.starts_with("https://") || rest.starts_with("http://")) && !marks.link {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let link = Marks {
            link: true,
            ..marks
        };
        return Some((end, Box::new(move |out| push(out, &rest[..end], link))));
    }
    // #tag, at a word's start.
    if let Some(inner) = rest.strip_prefix('#')
        && whole[..at]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    {
        let end = inner
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '/')))
            .unwrap_or(inner.len());
        if end > 0 && !inner[..end].chars().all(|c| c.is_ascii_digit()) {
            let tag = Marks { tag: true, ..marks };
            return Some((
                end + 1,
                Box::new(move |out| push(out, &rest[..end + 1], tag)),
            ));
        }
    }
    for (delimiter, mark) in PAIRS {
        let Some(inner) = rest.strip_prefix(delimiter) else {
            continue;
        };
        // `_` inside a word (snake_case) is not emphasis.
        if delimiter.starts_with('_')
            && whole[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        if inner.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(end) = closing(inner, delimiter) else {
            continue;
        };
        let mut inside = marks;
        mark(&mut inside);
        let body = &inner[..end];
        return Some((
            delimiter.len() * 2 + end,
            Box::new(move |out| walk(body, inside, out)),
        ));
    }
    None
}

/// Where `delimiter` closes in `inner`: not straight away, not after a
/// space, and for a single `*` not as the start of a `**`.
fn closing(inner: &str, delimiter: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = inner[from..].find(delimiter) {
        let end = from + found;
        let after = &inner[end + delimiter.len()..];
        let doubled = delimiter.len() == 1 && after.starts_with(delimiter);
        let before_space = inner[..end].ends_with(char::is_whitespace);
        if end > 0 && !before_space && !doubled {
            return Some(end);
        }
        from = end + delimiter.len() + usize::from(doubled);
        if from >= inner.len() {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(inline: &Inline) -> Vec<(&str, Marks)> {
        inline
            .spans
            .iter()
            .map(|(r, m)| (&inline.text[r.clone()], *m))
            .collect()
    }

    #[test]
    fn inline_markup_comes_off_and_marks_what_it_wrapped() {
        let line = inline("A **bold** and *it* with `co*de*` and ~~gone~~");
        assert_eq!(line.text, "A bold and it with co*de* and gone");
        let spans = marked(&line);
        assert_eq!(spans[0].0, "bold");
        assert!(spans[0].1.bold);
        assert_eq!(spans[1].0, "it");
        assert!(spans[1].1.italic);
        assert_eq!(spans[2].0, "co*de*");
        assert!(spans[2].1.code && !spans[2].1.italic);
        assert!(spans[3].1.strike);
    }

    #[test]
    fn nested_marks_never_overlap() {
        let line = inline("**bold *both* bold**");
        assert_eq!(line.text, "bold both bold");
        let spans = marked(&line);
        assert_eq!(spans.len(), 3);
        assert!(spans[1].1.bold && spans[1].1.italic);
        for pair in line.spans.windows(2) {
            assert!(pair[0].0.end <= pair[1].0.start);
        }
    }

    #[test]
    fn links_wiki_links_and_tags() {
        let line = inline("See [the doc](https://x.y) and [[Note|alias]] #work");
        assert_eq!(line.text, "See the doc and alias #work");
        let spans = marked(&line);
        assert!(spans[0].1.link && spans[0].0 == "the doc");
        assert!(spans[1].1.link && spans[1].0 == "alias");
        assert!(spans[2].1.tag && spans[2].0 == "#work");
        // A heading-like # mid-word, a number and snake_case stay plain.
        let plain = inline("issue#4 is #12 in snake_case_name");
        assert!(plain.spans.is_empty(), "{:?}", plain.spans);
    }

    #[test]
    fn unmatched_markers_are_left_as_typed() {
        let line = inline("2 * 3 = 6 and a lone ** here");
        assert_eq!(line.text, "2 * 3 = 6 and a lone ** here");
        assert!(line.spans.is_empty());
    }

    #[test]
    fn blocks_read_headings_items_quotes_code_and_properties() {
        let text = "---\ntags: [a]\n---\n# Title\n\n\n- one\n  - [x] done\n3. third\n> [!note] Heads up\n```rust\nlet x = 1;\n```\n---\nplain";
        let read = blocks(text);
        assert_eq!(read[0], Block::Properties("tags: [a]".into()));
        assert!(matches!(&read[1], Block::Heading { level: 1, text } if text.text == "Title"));
        assert_eq!(read[2], Block::Blank, "two blank lines draw as one gap");
        assert!(
            matches!(&read[3], Block::Item { depth: 0, task: None, marker, .. } if marker == "•")
        );
        assert!(
            matches!(&read[4], Block::Item { depth: 1, task: Some(true), text, .. } if text.text == "done")
        );
        assert!(matches!(&read[5], Block::Item { marker, .. } if marker == "3."));
        assert!(matches!(&read[6], Block::Quote(text) if text.text == "Heads up"));
        assert_eq!(
            read[7],
            Block::Code {
                lang: "rust".into(),
                text: "let x = 1;".into()
            }
        );
        assert_eq!(read[8], Block::Rule);
        assert!(matches!(&read[9], Block::Line(text) if text.text == "plain"));
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end() {
        let read = blocks("```\ncode\nmore");
        assert_eq!(
            read,
            vec![Block::Code {
                lang: String::new(),
                text: "code\nmore".into()
            }]
        );
    }
}
