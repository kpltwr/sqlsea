//! Suggestions for the word at the cursor in the SQL editor: tables, columns
//! and keywords. After `name.`, only the columns of that table, or of the
//! table that the alias `name` stands for.

use crate::db::{Table, ident};
use crate::guard;

/// The tables and the column names of each table.
pub struct Catalog<'a> {
    pub tables: &'a [Table],
    pub columns: &'a crate::ColumnIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// The typed part of the word, which a suggestion replaces.
    pub prefix: String,
    /// The text to insert for each suggestion.
    pub items: Vec<String>,
}

/// The most suggestions to show.
const LIMIT: usize = 8;

/// Suggestions for `line` with the cursor at byte `column`, in the SQL `text`.
pub fn complete(text: &str, line: &str, column: usize, catalog: &Catalog) -> Option<Completion> {
    let mut column = column.min(line.len());
    while !line.is_char_boundary(column) {
        column -= 1;
    }
    let (before, after) = line.split_at(column);
    // No suggestions in the middle of a word.
    if after.chars().next().is_some_and(is_word) {
        return None;
    }
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| !(is_word(*c) || *c == '.'))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let word = &before[start..];
    if word.is_empty() || word.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let (qualifier, prefix) = match word.rsplit_once('.') {
        Some((q, p)) => (Some(q), p),
        None => (None, word),
    };
    let needle = prefix.to_lowercase();
    let matches = |name: &str| {
        let lower = name.to_lowercase();
        lower.starts_with(&needle) && lower != needle
    };

    let mut items: Vec<String> = Vec::new();
    let push = |item: String, items: &mut Vec<String>| {
        if !items.contains(&item) {
            items.push(item);
        }
    };
    let mentioned = mentioned_tables(text);

    match qualifier {
        Some(q) => {
            let q = q.trim_matches('"').to_lowercase();
            // A schema: its tables.
            for t in catalog.tables {
                if t.schema.as_deref().is_some_and(|s| s.to_lowercase() == q) && matches(&t.name) {
                    push(name(&t.name), &mut items);
                }
            }
            // A table, or an alias of a table: its columns.
            let target = mentioned
                .iter()
                .find(|(_, alias)| alias.as_deref() == Some(q.as_str()))
                .map(|(table, _)| table.clone())
                .unwrap_or(q);
            for column in columns_of(&target, catalog) {
                if matches(column) {
                    push(name(column), &mut items);
                }
            }
        }
        None => {
            // Columns of the tables that the SQL uses come first.
            for (table, _) in &mentioned {
                for column in columns_of(table, catalog) {
                    if matches(column) {
                        push(name(column), &mut items);
                    }
                }
            }
            for t in catalog.tables {
                if matches(&t.name) {
                    push(name(&t.name), &mut items);
                }
            }
            if needle.len() >= 2 {
                let upper = prefix.chars().any(|c| c.is_uppercase())
                    && !prefix.chars().any(|c| c.is_lowercase());
                for keyword in crate::sql::KEYWORDS {
                    if matches(keyword) {
                        push(
                            if upper {
                                keyword.to_uppercase()
                            } else {
                                keyword.to_string()
                            },
                            &mut items,
                        );
                    }
                }
            }
        }
    }
    items.truncate(LIMIT);
    (!items.is_empty()).then(|| Completion {
        prefix: prefix.to_string(),
        items,
    })
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A name as SQL must write it: bare if it is plain lower case, else quoted.
fn name(name: &str) -> String {
    let plain = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && crate::sql::KEYWORDS.binary_search(&name).is_err();
    if plain { name.to_string() } else { ident(name) }
}

/// The columns of a table, found by name, with or without its schema.
fn columns_of<'a>(table: &str, catalog: &'a Catalog) -> &'a [String] {
    let table = table.trim_matches('"');
    let (schema, bare) = match table.rsplit_once('.') {
        Some((s, t)) => (Some(s.trim_matches('"')), t.trim_matches('"')),
        None => (None, table),
    };
    catalog
        .columns
        .iter()
        .find(|((s, t), _)| {
            t.eq_ignore_ascii_case(bare)
                && schema.is_none_or(|schema| {
                    s.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(schema))
                })
        })
        .map(|(_, columns)| columns.as_slice())
        .unwrap_or(&[])
}

/// The tables after FROM, JOIN, UPDATE and INTO, each with its alias if any.
fn mentioned_tables(text: &str) -> Vec<(String, Option<String>)> {
    const NOT_ALIAS: &[&str] = &[
        "where",
        "join",
        "on",
        "inner",
        "left",
        "right",
        "full",
        "cross",
        "natural",
        "order",
        "group",
        "limit",
        "offset",
        "set",
        "values",
        "using",
        "having",
        "union",
        "returning",
        "select",
        "from",
        "as",
    ];
    let mut out = Vec::new();
    for statement in guard::statements(text) {
        let words: Vec<&str> = statement.words.iter().map(|(w, _)| w.as_str()).collect();
        for (i, word) in words.iter().enumerate() {
            if !matches!(*word, "from" | "join" | "update" | "into") {
                continue;
            }
            let Some(table) = words.get(i + 1) else {
                continue;
            };
            if NOT_ALIAS.contains(table) {
                continue;
            }
            let mut j = i + 2;
            if words.get(j) == Some(&"as") {
                j += 1;
            }
            let alias = words
                .get(j)
                .filter(|w| !NOT_ALIAS.contains(*w))
                .map(|w| w.trim_matches('"').to_lowercase());
            out.push((table.to_lowercase(), alias));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn catalog() -> (Vec<Table>, crate::ColumnIndex) {
        let table = |s: &str, n: &str| Table {
            schema: Some(s.into()),
            name: n.into(),
            is_view: false,
        };
        let tables = vec![
            table("public", "customers"),
            table("public", "orders"),
            table("billing", "invoices"),
        ];
        let mut columns = HashMap::new();
        columns.insert(
            (Some("public".into()), "customers".into()),
            vec!["id".into(), "email".into(), "created_at".into()],
        );
        columns.insert(
            (Some("public".into()), "orders".into()),
            vec![
                "id".into(),
                "customer_id".into(),
                "status".into(),
                "CamelCase".into(),
            ],
        );
        (tables, columns)
    }

    fn run(text: &str) -> Option<Completion> {
        let (tables, columns) = catalog();
        let c = Catalog {
            tables: &tables,
            columns: &columns,
        };
        // The cursor is at the end of the last line.
        let line = text.lines().last().unwrap_or("");
        complete(text, line, line.len(), &c)
    }

    #[test]
    fn suggests_tables_and_keywords() {
        let c = run("select * from cu").unwrap();
        assert_eq!(c.prefix, "cu");
        assert!(c.items.contains(&"customers".to_string()));
        assert!(run("SEL").unwrap().items.contains(&"SELECT".to_string()));
    }

    #[test]
    fn suggests_columns_of_the_tables_in_use_first() {
        let c = run("select * from orders where st").unwrap();
        assert_eq!(c.items[0], "status");
    }

    #[test]
    fn follows_aliases_and_table_names() {
        let c = run("select * from orders o join customers c on c.").unwrap();
        assert_eq!(c.prefix, "");
        assert_eq!(c.items, ["id", "email", "created_at"]);
        let c = run("select * from orders o where o.cu").unwrap();
        assert_eq!(c.items, ["customer_id"]);
        let c = run("select * from orders where orders.st").unwrap();
        assert_eq!(c.items, ["status"]);
    }

    #[test]
    fn suggests_tables_of_a_schema() {
        assert_eq!(run("select * from billing.in").unwrap().items, ["invoices"]);
    }

    #[test]
    fn quotes_names_that_need_it() {
        let c = run("select * from orders where Cam").unwrap();
        assert_eq!(c.items, ["\"CamelCase\""]);
    }

    #[test]
    fn no_suggestion_for_a_whole_word_or_a_number() {
        assert!(run("select * from orders").is_none());
        assert!(run("select 12").is_none());
    }
}
