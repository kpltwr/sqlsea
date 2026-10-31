//! Writing result sets out as CSV or JSON.

use std::path::{Path, PathBuf};

use crate::db::ResultSet;

/// The system's save dialog, offering CSV and JSON.
pub async fn choose_file(name: &str) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Export")
        .set_file_name(format!("{name}.csv"))
        .add_filter("CSV", &["csv"])
        .add_filter("JSON", &["json"])
        .save_file()
        .await
        .map(|file| file.path().to_path_buf())
}

/// Writes JSON for a `.json` file and CSV for anything else.
pub fn write(path: &Path, rs: &ResultSet) -> Result<(), String> {
    let is_json = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    let body = if is_json { json(rs) } else { csv(rs) };
    std::fs::write(path, body).map_err(|e| e.to_string())
}

/// RFC 4180 CSV with a header row. NULL is an empty field.
pub fn csv(rs: &ResultSet) -> String {
    let mut out = String::new();
    let line = |out: &mut String, fields: &mut dyn Iterator<Item = Option<&str>>| {
        let mut first = true;
        for field in fields {
            if !first {
                out.push(',');
            }
            first = false;
            if let Some(value) = field {
                if value.contains([',', '"', '\n', '\r']) {
                    out.push('"');
                    out.push_str(&value.replace('"', "\"\""));
                    out.push('"');
                } else {
                    out.push_str(value);
                }
            }
        }
        out.push_str("\r\n");
    };
    line(&mut out, &mut rs.columns.iter().map(|c| Some(c.as_str())));
    for row in &rs.rows {
        line(&mut out, &mut row.iter().map(|v| v.as_deref()));
    }
    out
}

/// An array of objects keyed by column. Values stay text, as the database
/// gave them; NULL is `null`.
pub fn json(rs: &ResultSet) -> String {
    let rows: Vec<serde_json::Map<String, serde_json::Value>> = rs
        .rows
        .iter()
        .map(|row| {
            rs.columns
                .iter()
                .zip(row)
                .map(|(column, value)| {
                    let value = match value {
                        Some(v) => serde_json::Value::String(v.clone()),
                        None => serde_json::Value::Null,
                    };
                    (column.clone(), value)
                })
                .collect()
        })
        .collect();
    serde_json::to_string_pretty(&rows).unwrap_or_default()
}

/// One row as tab-separated values, for pasting into a spreadsheet.
pub fn tsv_row(row: &[Option<String>]) -> String {
    row.iter()
        .map(|v| v.as_deref().unwrap_or("").replace(['\t', '\n', '\r'], " "))
        .collect::<Vec<_>>()
        .join("\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ResultSet {
        ResultSet {
            columns: vec!["id".into(), "note".into()],
            rows: vec![
                vec![Some("1".into()), Some("plain".into())],
                vec![
                    Some("2".into()),
                    Some("has \"quotes\", a comma\nand a line".into()),
                ],
                vec![Some("3".into()), None],
            ],
            ..Default::default()
        }
    }

    #[test]
    fn csv_quotes_only_what_needs_it() {
        assert_eq!(
            csv(&sample()),
            "id,note\r\n1,plain\r\n2,\"has \"\"quotes\"\", a comma\nand a line\"\r\n3,\r\n"
        );
    }

    #[test]
    fn json_keeps_nulls() {
        let v: serde_json::Value = serde_json::from_str(&json(&sample())).unwrap();
        assert_eq!(v[0]["note"], "plain");
        assert!(v[2]["note"].is_null());
    }

    #[test]
    fn tsv_flattens_tabs_and_newlines() {
        assert_eq!(
            tsv_row(&[Some("a\tb".into()), None, Some("c\nd".into())]),
            "a b\t\tc d"
        );
    }
}
