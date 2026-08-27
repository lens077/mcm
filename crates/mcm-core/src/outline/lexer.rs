//! Line classification and annotation tokenisation for the outline grammar
//! (contracts/outline-grammar.md §行类型 / §任务行注解).
//!
//! The lexer never fails: malformed input is reported as a typed `LexIssue`
//! so the parser can quarantine the line and keep going (spec FR-015).

/// One physical source line, classified but not yet validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    Blank,
    /// `%mcm 1`
    Version {
        raw: String,
        value: String,
    },
    /// `%title ...`, `%start ...`, `%desc ...`, or an unknown directive.
    Directive {
        name: String,
        value: String,
    },
    /// `# comment`
    Comment {
        text: String,
    },
    /// `<indent>- [x] title #id [dates] @owner <-t1`
    Task {
        indent: usize,
        done: Option<bool>,
        body: String,
    },
    /// `<indent>> note text`
    Note {
        indent: usize,
        text: String,
    },
    /// `! name #id [date] <-t1`
    Milestone {
        indent: usize,
        body: String,
    },
    /// Anything that does not match a known shape.
    Unknown {
        raw: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLine {
    /// 1-based line number used by `P-*` issue locations.
    pub number: u32,
    pub raw: String,
    pub kind: LineKind,
    /// Indentation problems detected while measuring (`P-002`).
    pub indent_error: Option<IndentError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentError {
    /// Indentation was not a multiple of two spaces.
    NotMultipleOfTwo { spaces: usize },
    /// A tab character was used for indentation.
    TabUsed,
}

/// Measures leading indentation in two-space levels.
fn measure_indent(line: &str) -> (usize, usize, Option<IndentError>) {
    let mut spaces = 0usize;
    let mut tab_used = false;
    for ch in line.chars() {
        match ch {
            ' ' => spaces += 1,
            '\t' => {
                tab_used = true;
                spaces += 1;
            }
            _ => break,
        }
    }
    let offset = spaces;
    if tab_used {
        return (spaces / 2, offset, Some(IndentError::TabUsed));
    }
    if spaces % 2 != 0 {
        return (
            spaces / 2,
            offset,
            Some(IndentError::NotMultipleOfTwo { spaces }),
        );
    }
    (spaces / 2, offset, None)
}

/// Splits a document into classified lines. CRLF is tolerated (normalised on save).
#[must_use]
pub fn lex(source: &str) -> Vec<SourceLine> {
    source
        .lines()
        .enumerate()
        .map(|(index, raw_line)| {
            let raw = raw_line.strip_suffix('\r').unwrap_or(raw_line).to_owned();
            let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
            classify(number, raw)
        })
        .collect()
}

fn classify(number: u32, raw: String) -> SourceLine {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return SourceLine {
            number,
            raw,
            kind: LineKind::Blank,
            indent_error: None,
        };
    }

    if let Some(rest) = trimmed.strip_prefix('%') {
        let (name, value) = split_first_word(rest);
        let kind = if name == "mcm" {
            LineKind::Version {
                raw: raw.clone(),
                value: value.to_owned(),
            }
        } else {
            LineKind::Directive {
                name: name.to_owned(),
                value: value.to_owned(),
            }
        };
        return SourceLine {
            number,
            raw,
            kind,
            indent_error: None,
        };
    }

    if let Some(text) = trimmed.strip_prefix('#') {
        let kind = LineKind::Comment {
            text: text.trim().to_owned(),
        };
        return SourceLine {
            number,
            raw,
            kind,
            indent_error: None,
        };
    }

    let (indent, _offset, indent_error) = measure_indent(&raw);

    if let Some(rest) = trimmed
        .strip_prefix("! ")
        .or_else(|| trimmed.strip_prefix('!'))
    {
        let kind = LineKind::Milestone {
            indent,
            body: rest.trim().to_owned(),
        };
        return SourceLine {
            number,
            raw,
            kind,
            indent_error,
        };
    }

    if let Some(rest) = trimmed
        .strip_prefix("> ")
        .or_else(|| trimmed.strip_prefix('>'))
    {
        let kind = LineKind::Note {
            indent,
            text: rest.to_owned(),
        };
        return SourceLine {
            number,
            raw,
            kind,
            indent_error,
        };
    }

    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix('-'))
    {
        let rest = rest.trim_start();
        let (done, body) = if let Some(after) = rest.strip_prefix("[x]") {
            (Some(true), after.trim_start())
        } else if let Some(after) = rest.strip_prefix("[X]") {
            (Some(true), after.trim_start())
        } else if let Some(after) = rest.strip_prefix("[ ]") {
            (Some(false), after.trim_start())
        } else {
            (None, rest)
        };
        let kind = LineKind::Task {
            indent,
            done,
            body: body.to_owned(),
        };
        return SourceLine {
            number,
            raw,
            kind,
            indent_error,
        };
    }

    let unknown = LineKind::Unknown {
        raw: trimmed.to_owned(),
    };
    SourceLine {
        number,
        raw,
        kind: unknown,
        indent_error,
    }
}

fn split_first_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    match text.find(char::is_whitespace) {
        Some(index) => (&text[..index], text[index..].trim()),
        None => (text, ""),
    }
}

/// One whitespace-separated token from a task/milestone body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A plain title word (escapes already resolved).
    Word(String),
    /// `#t3` / `#m1`
    Id(String),
    /// `[...]` payload without the brackets.
    Bracket(String),
    /// `@name`
    Assignee(String),
    /// `<-t3`
    Predecessor(String),
}

/// Splits a body into annotation tokens. Escaped leading markers (`\#`) become
/// plain words so titles can contain literal `#`, `@`, `[` and `<-`.
#[must_use]
pub fn tokenize_body(body: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut start = None::<usize>;
    let mut depth = 0usize;
    let mut in_quotes = false;

    // Bracket payloads may contain spaces, so scan manually instead of split_whitespace.
    let mut index = 0usize;
    while index < body.len() {
        let ch = body[index..].chars().next().unwrap_or(' ');
        let ch_len = ch.len_utf8();
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == '[' {
            depth += 1;
        } else if ch == ']' && depth > 0 {
            depth -= 1;
        }
        let is_boundary = ch.is_whitespace() && depth == 0 && !in_quotes;
        if is_boundary {
            if let Some(begin) = start.take() {
                tokens.push(classify_token(&body[begin..index]));
            }
        } else if start.is_none() {
            start = Some(index);
        }
        index += ch_len;
    }
    if let Some(begin) = start {
        tokens.push(classify_token(&body[begin..]));
    }
    tokens
}

fn classify_token(text: &str) -> Token {
    if let Some(rest) = text.strip_prefix("\\") {
        // Escaped marker: keep the literal text as a title word.
        return Token::Word(rest.to_owned());
    }
    if let Some(rest) = text.strip_prefix("<-") {
        return Token::Predecessor(rest.to_owned());
    }
    if let Some(rest) = text.strip_prefix('#') {
        return Token::Id(rest.to_owned());
    }
    if let Some(rest) = text.strip_prefix('@') {
        // `@"名 字"` carries an assignee containing spaces.
        let unquoted = rest
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(rest);
        return Token::Assignee(unquoted.to_owned());
    }
    if let Some(rest) = text.strip_prefix('[') {
        let inner = rest.strip_suffix(']').unwrap_or(rest);
        return Token::Bracket(inner.to_owned());
    }
    // A quoted word carries interior whitespace verbatim.
    if let Some(inner) = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return Token::Word(inner.to_owned());
    }
    Token::Word(text.to_owned())
}

/// True when re-splitting the title on single spaces would not reproduce it
/// (leading/trailing space, or runs of two or more spaces).
fn needs_quoting(title: &str) -> bool {
    title != title.trim() || title.contains("  ")
}

/// Adds escapes/quotes so a title round-trips through `tokenize_body` unchanged.
#[must_use]
pub fn escape_title(title: &str) -> String {
    if needs_quoting(title) {
        // Quote the whole title so exact spacing survives the round trip.
        return format!("\"{title}\"");
    }
    title
        .split(' ')
        .map(|word| {
            if word.starts_with('#')
                || word.starts_with('@')
                || word.starts_with('[')
                || word.starts_with("<-")
                || word.starts_with('\\')
                || word.starts_with('"')
            {
                format!("\\{word}")
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_every_line_type() {
        let lines =
            lex("%mcm 1\n%title 移动端改版\n# 注释\n- 任务 #t1\n  > 备注\n! 冻结 [2026-09-10]\n\n");
        assert!(matches!(lines[0].kind, LineKind::Version { .. }));
        assert!(matches!(&lines[1].kind, LineKind::Directive { name, .. } if name == "title"));
        assert!(matches!(&lines[2].kind, LineKind::Comment { text } if text == "注释"));
        assert!(matches!(&lines[3].kind, LineKind::Task { indent: 0, .. }));
        assert!(matches!(&lines[4].kind, LineKind::Note { indent: 1, .. }));
        assert!(matches!(&lines[5].kind, LineKind::Milestone { .. }));
        assert!(matches!(lines[6].kind, LineKind::Blank));
    }

    #[test]
    fn measures_two_space_indent_levels() {
        let lines = lex("- a\n  - b\n    - c\n");
        let indents: Vec<usize> = lines
            .iter()
            .filter_map(|line| match line.kind {
                LineKind::Task { indent, .. } => Some(indent),
                _ => None,
            })
            .collect();
        assert_eq!(indents, vec![0, 1, 2]);
    }

    #[test]
    fn flags_odd_indentation_and_tabs() {
        let lines = lex("   - a\n\t- b\n");
        assert_eq!(
            lines[0].indent_error,
            Some(IndentError::NotMultipleOfTwo { spaces: 3 })
        );
        assert_eq!(lines[1].indent_error, Some(IndentError::TabUsed));
    }

    #[test]
    fn parses_done_markers() {
        let lines = lex("- [x] 完成\n- [ ] 未完成\n- 普通\n");
        let dones: Vec<Option<bool>> = lines
            .iter()
            .filter_map(|line| match line.kind {
                LineKind::Task { done, .. } => Some(done),
                _ => None,
            })
            .collect();
        assert_eq!(dones, vec![Some(true), Some(false), None]);
    }

    #[test]
    fn tokenizes_annotations() {
        let tokens = tokenize_body("竞品分析 #t3 [2026-09-01..2026-09-05] @王芳 <-t2");
        assert_eq!(tokens[0], Token::Word("竞品分析".into()));
        assert_eq!(tokens[1], Token::Id("t3".into()));
        assert_eq!(tokens[2], Token::Bracket("2026-09-01..2026-09-05".into()));
        assert_eq!(tokens[3], Token::Assignee("王芳".into()));
        assert_eq!(tokens[4], Token::Predecessor("t2".into()));
    }

    #[test]
    fn annotations_may_appear_between_title_words() {
        let tokens = tokenize_body("前 #t1 中 @人 后");
        let words: Vec<&str> = tokens
            .iter()
            .filter_map(|t| match t {
                Token::Word(w) => Some(w.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(words, vec!["前", "中", "后"]);
    }

    #[test]
    fn escaped_markers_stay_title_words() {
        let tokens = tokenize_body("\\#hashtag \\@handle");
        assert_eq!(tokens[0], Token::Word("#hashtag".into()));
        assert_eq!(tokens[1], Token::Word("@handle".into()));
    }

    #[test]
    fn escape_title_round_trips() {
        let title = "#hashtag 普通 @handle";
        let escaped = escape_title(title);
        let tokens = tokenize_body(&escaped);
        let words: Vec<String> = tokens
            .iter()
            .filter_map(|t| match t {
                Token::Word(w) => Some(w.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(words.join(" "), title);
    }

    #[test]
    fn titles_with_repeated_spaces_round_trip_via_quoting() {
        // Regression: `一  a` used to collapse to `一 a` on re-serialization.
        for title in ["一  a", " 前导空格", "尾随空格 ", "多个   空格"] {
            let escaped = escape_title(title);
            let tokens = tokenize_body(&escaped);
            let words: Vec<String> = tokens
                .iter()
                .filter_map(|t| match t {
                    Token::Word(w) => Some(w.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                words.join(" "),
                title,
                "failed for {title:?} (escaped: {escaped:?})"
            );
        }
    }

    #[test]
    fn quoted_titles_keep_interior_spacing() {
        let tokens = tokenize_body("\"保留  空格\" #t1");
        assert_eq!(tokens[0], Token::Word("保留  空格".into()));
        assert_eq!(tokens[1], Token::Id("t1".into()));
    }

    #[test]
    fn tolerates_crlf() {
        let lines = lex("%mcm 1\r\n- 任务\r\n");
        assert!(matches!(lines[0].kind, LineKind::Version { .. }));
        assert!(!lines[0].raw.contains('\r'));
    }

    #[test]
    fn unknown_lines_are_preserved() {
        let lines = lex("这是一行乱码\n");
        assert!(matches!(&lines[0].kind, LineKind::Unknown { raw } if raw == "这是一行乱码"));
    }
}
