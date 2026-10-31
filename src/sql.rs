//! A small line-based SQL highlighter for the query editor. Keywords take the
//! cobalt tint; strings a softer one; comments recede. No rainbow.

use std::ops::Range;

use iced::advanced::text::highlighter::{self, Highlighter};
use iced::{Font, Theme};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Keyword,
    Literal,
    Comment,
}

pub struct Sql {
    line: usize,
}

const KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "any",
    "as",
    "asc",
    "begin",
    "between",
    "by",
    "case",
    "cast",
    "check",
    "column",
    "commit",
    "constraint",
    "create",
    "cross",
    "database",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "except",
    "exists",
    "explain",
    "false",
    "foreign",
    "from",
    "full",
    "group",
    "having",
    "if",
    "ilike",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "limit",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "pragma",
    "primary",
    "references",
    "returning",
    "right",
    "rollback",
    "schema",
    "select",
    "set",
    "table",
    "then",
    "true",
    "truncate",
    "union",
    "unique",
    "update",
    "using",
    "values",
    "view",
    "when",
    "where",
    "with",
];

impl Highlighter for Sql {
    type Settings = ();
    type Highlight = Token;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Token)>;

    fn new(_: &Self::Settings) -> Self {
        Sql { line: 0 }
    }

    fn update(&mut self, _: &Self::Settings) {
        self.line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.line += 1;
        tokens(line).into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

fn tokens(line: &str) -> Vec<(Range<usize>, Token)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'-' && bytes.get(i + 1) == Some(&b'-') {
            out.push((i..bytes.len(), Token::Comment));
            break;
        } else if c == b'\'' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if bytes.get(i + 1) == Some(&b'\'') {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push((start..i, Token::Literal));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            out.push((start..i, Token::Literal));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = line[start..i].to_ascii_lowercase();
            if KEYWORDS.binary_search(&word.as_str()).is_ok() {
                out.push((start..i, Token::Keyword));
            }
        } else if c == b'"' {
            // Quoted identifiers stay plain; skip so their contents are not read as keywords.
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

pub fn format(token: &Token, t: &Theme) -> highlighter::Format<Font> {
    highlighter::Format {
        color: Some(match token {
            Token::Keyword => theme::colors(t).accent2,
            Token::Literal => theme::colors(t).accent_soft,
            Token::Comment => theme::colors(t).graphite_muted,
        }),
        font: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_are_sorted() {
        assert!(KEYWORDS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn tokenizes() {
        let t = tokens("SELECT 'it''s', 42 from \"select\" -- note");
        let kinds: Vec<Token> = t.iter().map(|(_, k)| *k).collect();
        assert_eq!(
            kinds,
            [
                Token::Keyword,
                Token::Literal,
                Token::Literal,
                Token::Keyword,
                Token::Comment
            ]
        );
    }
}
