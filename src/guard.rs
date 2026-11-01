//! Finds the statements in SQL text, and the statements that change or remove
//! every row of a table. The Query tab asks before it runs those statements.

/// One statement, with its words outside strings and comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub text: String,
    /// Each word in lower case, with its bracket depth. Quoted names keep
    /// their quotes, so that they are never read as keywords.
    pub words: Vec<(String, u32)>,
}

/// Splits SQL into statements at semicolons that are not in a string, a
/// quoted name or a comment. Empty statements are left out.
pub fn statements(sql: &str) -> Vec<Statement> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    let mut words = Vec::new();
    let mut depth: u32 = 0;
    let mut i = 0;

    let finish =
        |from: usize, end: usize, words: &mut Vec<(String, u32)>, out: &mut Vec<Statement>| {
            let text: String = chars[from..end.max(from)].iter().collect();
            if !words.is_empty() {
                out.push(Statement {
                    text: text.trim().to_string(),
                    words: std::mem::take(words),
                });
            }
        };

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '-' if next == Some('-') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if next == Some('*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            '\'' => {
                // `E'...'` strings use backslash escapes.
                let escapes = i > 0 && matches!(chars[i - 1], 'e' | 'E');
                i += 1;
                while i < chars.len() {
                    if escapes && chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            '"' | '`' | '[' => {
                let close = if c == '[' { ']' } else { c };
                let begin = i;
                i += 1;
                while i < chars.len() && chars[i] != close {
                    i += 1;
                }
                i += 1;
                let word: String = chars[begin..i.min(chars.len())].iter().collect();
                words.push((word, depth));
            }
            '$' => {
                // A PostgreSQL dollar-quoted string: `$$...$$` or `$tag$...$tag$`.
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                if j < chars.len() && chars[j] == '$' {
                    let tag: String = chars[i..=j].iter().collect();
                    let tag: Vec<char> = tag.chars().collect();
                    i = j + 1;
                    while i < chars.len() && !chars[i..].starts_with(&tag) {
                        i += 1;
                    }
                    i += tag.len();
                } else {
                    i = j;
                }
            }
            '(' => {
                depth += 1;
                i += 1;
            }
            ')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            ';' => {
                finish(start, i, &mut words, &mut out);
                start = i + 1;
                depth = 0;
                i += 1;
            }
            c if c.is_alphanumeric() || c == '_' => {
                let begin = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                let word: String = chars[begin..i].iter().collect::<String>().to_lowercase();
                words.push((word, depth));
            }
            _ => i += 1,
        }
    }
    finish(start, chars.len(), &mut words, &mut out);
    out
}

/// For each statement that changes or removes every row of a table, a plain
/// sentence that says so. A statement with a `WHERE` clause is not listed.
pub fn risks(sql: &str) -> Vec<String> {
    statements(sql).iter().filter_map(risk).collect()
}

fn risk(statement: &Statement) -> Option<String> {
    let top: Vec<&str> = statement
        .words
        .iter()
        .filter(|(_, depth)| *depth == 0)
        .map(|(w, _)| w.as_str())
        .collect();
    // After `WITH ... AS (...)`, the main verb is the first top-level verb.
    let verb_at = top.iter().position(|w| {
        matches!(
            *w,
            "delete" | "update" | "truncate" | "drop" | "select" | "insert"
        )
    })?;
    let rest = &top[verb_at + 1..];
    let has_where = rest.contains(&"where");
    let after = |keyword: &str| {
        rest.iter()
            .skip_while(|w| **w != keyword)
            .nth(1)
            .map(|w| w.to_string())
    };
    match top[verb_at] {
        "delete" if !has_where => {
            let table = after("from").unwrap_or_else(|| "the table".into());
            Some(format!(
                "This DELETE has no WHERE clause, so it removes every row in {table}."
            ))
        }
        "update" if !has_where => {
            let table = rest
                .iter()
                .find(|w| **w != "only")
                .map(|w| w.to_string())
                .unwrap_or_else(|| "the table".into());
            Some(format!(
                "This UPDATE has no WHERE clause, so it changes every row in {table}."
            ))
        }
        "truncate" => {
            let table = rest
                .iter()
                .find(|w| !matches!(**w, "table" | "only"))
                .map(|w| w.to_string())
                .unwrap_or_else(|| "the table".into());
            Some(format!("This TRUNCATE removes every row in {table}."))
        }
        "drop" => {
            let kind = *rest.first()?;
            if !matches!(kind, "table" | "schema" | "database") {
                return None;
            }
            let name = rest[1..]
                .iter()
                .find(|w| !matches!(**w, "if" | "exists"))
                .map(|w| w.to_string())
                .unwrap_or_else(|| format!("the {kind}"));
            Some(format!(
                "This DROP removes the {kind} {name} and all of its data."
            ))
        }
        _ => None,
    }
}

/// True if every statement only reads data, so that it is safe to run again.
/// When in doubt, the answer is false.
pub fn reads_only(sql: &str) -> bool {
    const WRITES: &[&str] = &[
        "insert",
        "update",
        "delete",
        "merge",
        "upsert",
        "replace",
        "create",
        "drop",
        "alter",
        "truncate",
        "grant",
        "revoke",
        "copy",
        "call",
        "do",
        "vacuum",
        "analyze",
        "reindex",
        "refresh",
        "lock",
        "set",
        "reset",
        "begin",
        "start",
        "commit",
        "rollback",
        "savepoint",
        "attach",
        "detach",
        "pragma",
        "into",
        "for",
        "nextval",
        "setval",
    ];
    let statements = statements(sql);
    !statements.is_empty()
        && statements.iter().all(|statement| {
            let first = statement.words.first().map(|(w, _)| w.as_str());
            matches!(first, Some("select" | "with" | "values" | "table" | "show"))
                // At any depth: `with x as (delete ...)` writes, and
                // `select ... for update` locks rows.
                && !statement.words.iter().any(|(w, _)| WRITES.contains(&w.as_str()))
        })
}

/// True if the text is one statement at most. A filter must not hide a
/// second statement.
pub fn is_single(sql: &str) -> bool {
    statements(sql).len() <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_only_outside_strings_and_comments() {
        let s = statements(
            "select ';' as a; -- a comment; with a semicolon\n\
             select $$ x; y $$; /* ; */ select \"a;b\" from t;",
        );
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].text, "select ';' as a");
        assert!(s[2].words.iter().any(|(w, _)| w == "\"a;b\""));
    }

    #[test]
    fn finds_changes_to_every_row() {
        assert_eq!(
            risks("delete from orders"),
            ["This DELETE has no WHERE clause, so it removes every row in orders."]
        );
        assert_eq!(
            risks("UPDATE public.users SET admin = true"),
            ["This UPDATE has no WHERE clause, so it changes every row in public.users."]
        );
        assert_eq!(risks("truncate table logs").len(), 1);
        assert_eq!(risks("drop table if exists old_data").len(), 1);
    }

    #[test]
    fn leaves_safe_statements_alone() {
        assert!(risks("delete from orders where id = 4").is_empty());
        assert!(risks("update t set a = 1 where b = 2").is_empty());
        assert!(risks("select * from t").is_empty());
        assert!(risks("drop index idx_orders").is_empty());
        assert!(risks("insert into t values (1)").is_empty());
        // Words in strings and comments are not keywords.
        assert!(risks("select 'delete from t'").is_empty());
        assert!(risks("-- delete from t\nselect 1").is_empty());
    }

    #[test]
    fn a_where_in_a_subquery_does_not_count() {
        assert_eq!(
            risks("update t set a = (select max(b) from u where u.id = 1)").len(),
            1
        );
    }

    #[test]
    fn finds_the_verb_after_a_with_clause() {
        assert_eq!(
            risks("with old as (select id from t where x < 1) delete from t").len(),
            1
        );
        assert!(
            risks("with old as (select id from t) delete from t where id in (select id from old)")
                .is_empty()
        );
    }

    #[test]
    fn knows_which_queries_only_read() {
        assert!(reads_only("select * from orders where status = 'update'"));
        assert!(reads_only(
            "with t as (select 1) select * from t; values (1), (2)"
        ));
        assert!(reads_only("-- delete from t\nselect 1"));
        assert!(!reads_only("insert into t values (1) returning id"));
        assert!(!reads_only(
            "with gone as (delete from t returning *) select * from gone"
        ));
        assert!(!reads_only("select * into backup from t"));
        assert!(!reads_only("select * from t for update"));
        assert!(!reads_only("select nextval('s')"));
        assert!(!reads_only("select 1; update t set a = 1"));
        assert!(!reads_only("explain analyze delete from t"));
        assert!(!reads_only(""));
    }

    #[test]
    fn one_condition_is_single() {
        assert!(is_single("status = 'a;b'"));
        assert!(!is_single("1 = 1; delete from t"));
    }
}
