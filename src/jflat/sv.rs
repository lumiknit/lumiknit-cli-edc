//! Separated-value (CSV/TSV) support for jflat.
//!
//! Rows become array elements; columns become either object keys (header
//! mode, the default, keyed by the header row) or object keys "0", "1", ...
//! (no-header mode). A pure `array of arrays` shape is intentionally not
//! produced on flatten, but is still accepted on unflatten so hand-written
//! JSON/YAML/TOML can also be converted to SV.

use super::{Entry, Seg};
use crate::jsonlite;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvOpts {
    pub sep: char,
    pub quote: bool,
    pub header: bool,
}

impl SvOpts {
    pub fn csv() -> Self {
        SvOpts {
            sep: ',',
            quote: true,
            header: true,
        }
    }

    pub fn tsv() -> Self {
        SvOpts {
            sep: '\t',
            quote: false,
            header: true,
        }
    }
}

// ---- Parse SV text into rows of raw fields ---------------------------------

fn parse_rows(input: &str, opts: &SvOpts) -> Result<Vec<Vec<String>>, String> {
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut rows: Vec<Vec<String>> = vec![];
    let mut row: Vec<String> = vec![];
    let mut field = String::new();

    while i < n {
        let c = chars[i];
        if opts.quote && c == '"' && field.is_empty() {
            i += 1;
            loop {
                if i >= n {
                    return Err("unterminated quoted field".to_string());
                }
                if chars[i] == '"' {
                    if i + 1 < n && chars[i + 1] == '"' {
                        field.push('"');
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    field.push(chars[i]);
                    i += 1;
                }
            }
        } else if c == opts.sep {
            row.push(std::mem::take(&mut field));
            i += 1;
        } else if c == '\r' || c == '\n' {
            i += 1;
            if c == '\r' && i < n && chars[i] == '\n' {
                i += 1;
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
        } else {
            field.push(c);
            i += 1;
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

// ---- Flatten ----------------------------------------------------------------

pub fn flatten_sv(
    input: &str,
    opts: &SvOpts,
    path: &mut Vec<Seg>,
    out: &mut Vec<Entry>,
) -> Result<(), String> {
    let mut rows = parse_rows(input, opts)?;
    if rows.is_empty() {
        out.push(Entry {
            path: path.clone(),
            typ: "array",
            val: "[]".into(),
        });
        return Ok(());
    }

    let header: Option<Vec<String>> =
        if opts.header { Some(rows.remove(0)) } else { None };

    for (i, row) in rows.iter().enumerate() {
        path.push(Seg::Idx(i));
        if row.is_empty() {
            out.push(Entry {
                path: path.clone(),
                typ: "object",
                val: "{}".into(),
            });
        } else {
            for (j, cell) in row.iter().enumerate() {
                let key = match &header {
                    Some(h) => h.get(j).cloned().unwrap_or_else(|| j.to_string()),
                    None => j.to_string(),
                };
                path.push(Seg::Key(key));
                out.push(Entry {
                    path: path.clone(),
                    typ: "string",
                    val: jsonlite::quote(cell),
                });
                path.pop();
            }
        }
        path.pop();
    }
    Ok(())
}

// ---- Unflatten ----------------------------------------------------------------

fn cell_to_string(v: &serde_json::Value) -> Result<String, String> {
    Ok(match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            return Err(
                "nested object/array cannot be represented in SV output"
                    .to_string(),
            );
        }
    })
}

fn encode_field(s: &str, opts: &SvOpts) -> Result<String, String> {
    let needs_quote =
        s.contains(opts.sep) || s.contains('"') || s.contains('\n') || s.contains('\r');
    if !needs_quote {
        return Ok(s.to_string());
    }
    if !opts.quote {
        return Err(format!(
            "field {s:?} contains the separator, a quote, or a newline, \
             but quoting is disabled for this SV format"
        ));
    }
    Ok(format!("\"{}\"", s.replace('"', "\"\"")))
}

fn encode_row(cells: &[String], opts: &SvOpts) -> Result<String, String> {
    let fields: Result<Vec<String>, String> =
        cells.iter().map(|c| encode_field(c, opts)).collect();
    Ok(format!("{}\n", fields?.join(&opts.sep.to_string())))
}

pub fn unflatten_sv(
    root: &serde_json::Value,
    opts: &SvOpts,
) -> Result<String, String> {
    // A top-level array is treated as rows. A top-level object is treated
    // as a single-row array. A top-level atom (null/bool/number/string) is
    // written out directly as a single cell.
    let owned_rows;
    let rows: &[serde_json::Value] = match root {
        serde_json::Value::Null => &[],
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(_) => {
            owned_rows = [root.clone()];
            &owned_rows
        }
        atom => {
            if opts.header {
                return Err(
                    "header requested but top-level value is a scalar, not rows"
                        .to_string(),
                );
            }
            let cell = cell_to_string(atom)?;
            return encode_row(&[cell], opts);
        }
    };

    let mut columns: Vec<String> = vec![];
    let mut saw_object = false;
    for row in rows {
        match row {
            serde_json::Value::Object(map) => {
                saw_object = true;
                for k in map.keys() {
                    if !columns.contains(k) {
                        columns.push(k.clone());
                    }
                }
            }
            serde_json::Value::Array(_) | serde_json::Value::Null => {}
            _ => {
                return Err(
                    "each SV row must be an object or an array".to_string()
                );
            }
        }
    }
    if saw_object && !opts.header {
        // Default (no-header) shape uses "0", "1", ... string keys; order
        // them numerically so columns come out in the original order.
        columns.sort_by_key(|k| k.parse::<usize>().unwrap_or(usize::MAX));
    }

    let mut out = String::new();
    if opts.header {
        if !saw_object {
            return Err(
                "-header/header output requires object rows with column names"
                    .to_string(),
            );
        }
        out.push_str(&encode_row(&columns, opts)?);
    }

    for row in rows {
        let cells: Vec<String> = match row {
            serde_json::Value::Object(map) => columns
                .iter()
                .map(|c| match map.get(c) {
                    Some(v) => cell_to_string(v),
                    None => Ok(String::new()),
                })
                .collect::<Result<_, _>>()?,
            serde_json::Value::Array(arr) => {
                arr.iter().map(cell_to_string).collect::<Result<_, _>>()?
            }
            serde_json::Value::Null => vec![],
            _ => unreachable!(),
        };
        out.push_str(&encode_row(&cells, opts)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flatten(input: &str, opts: &SvOpts) -> Vec<Entry> {
        let mut path = vec![];
        let mut out = vec![];
        flatten_sv(input, opts, &mut path, &mut out).expect("flatten failed");
        out
    }

    #[test]
    fn csv_no_header() {
        let mut opts = SvOpts::csv();
        opts.header = false;
        let entries = flatten("a,b\n1,2\n", &opts);
        assert_eq!(entries.len(), 4);
        assert_eq!(super::super::fmt_path(&entries[0].path, "\t"), "[0].0");
        assert_eq!(entries[0].val, "\"a\"");
        assert_eq!(super::super::fmt_path(&entries[3].path, "\t"), "[1].1");
        assert_eq!(entries[3].val, "\"2\"");
    }

    #[test]
    fn csv_header() {
        // header is on by default
        let entries = flatten("name,age\nal,30\n", &SvOpts::csv());
        assert_eq!(entries.len(), 2);
        assert_eq!(super::super::fmt_path(&entries[0].path, "\t"), "[0].name");
        assert_eq!(entries[0].val, "\"al\"");
    }

    #[test]
    fn csv_quoted_field_with_comma_and_escaped_quote() {
        let mut opts = SvOpts::csv();
        opts.header = false;
        let entries = flatten("a,b\n\"x,y\",\"he said \"\"hi\"\"\"\n", &opts);
        assert_eq!(entries[2].val, "\"x,y\"");
        assert_eq!(entries[3].val, "\"he said \\\"hi\\\"\"");
    }

    #[test]
    fn tsv_disallows_quotes_by_default() {
        let mut opts = SvOpts::tsv();
        opts.header = false;
        let entries = flatten("a\tb\n\"x\"\ty\n", &opts);
        // With quoting disabled the quote chars are taken literally.
        assert_eq!(entries[2].val, "\"\\\"x\\\"\"");
    }

    #[test]
    fn unflatten_roundtrip_no_header() {
        let json: serde_json::Value =
            serde_json::from_str(r#"[{"0":"a","1":"b"},{"0":"1","1":"2"}]"#)
                .unwrap();
        let mut opts = SvOpts::csv();
        opts.header = false;
        let sv = unflatten_sv(&json, &opts).unwrap();
        assert_eq!(sv, "a,b\n1,2\n");
    }

    #[test]
    fn unflatten_with_header() {
        let json: serde_json::Value =
            serde_json::from_str(r#"[{"name":"al","age":30}]"#).unwrap();
        // header is on by default
        let sv = unflatten_sv(&json, &SvOpts::csv()).unwrap();
        // serde_json::Map has no preserve_order feature enabled here, so
        // keys come out sorted (matches the rest of jflat's JSON handling).
        assert_eq!(sv, "age,name\n30,al\n");
    }

    #[test]
    fn unflatten_rejects_nested_value() {
        let json: serde_json::Value =
            serde_json::from_str(r#"[{"a":[1,2]}]"#).unwrap();
        assert!(unflatten_sv(&json, &SvOpts::csv()).is_err());
    }

    #[test]
    fn unflatten_atom_becomes_single_cell() {
        let mut opts = SvOpts::csv();
        opts.header = false;
        let json: serde_json::Value = serde_json::from_str("42").unwrap();
        assert_eq!(unflatten_sv(&json, &opts).unwrap(), "42\n");

        let json: serde_json::Value =
            serde_json::from_str(r#""hi,there""#).unwrap();
        assert_eq!(unflatten_sv(&json, &opts).unwrap(), "\"hi,there\"\n");
    }

    #[test]
    fn unflatten_atom_rejects_header() {
        let json: serde_json::Value = serde_json::from_str("42").unwrap();
        let mut opts = SvOpts::csv();
        opts.header = true;
        assert!(unflatten_sv(&json, &opts).is_err());
    }

    #[test]
    fn unflatten_top_level_object_becomes_single_row() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"name":"al","age":30}"#).unwrap();
        let mut opts = SvOpts::csv();
        opts.header = true;
        assert_eq!(
            unflatten_sv(&json, &opts).unwrap(),
            "age,name\n30,al\n"
        );
    }

    #[test]
    fn unflatten_rejects_unquotable_field_for_tsv() {
        let json: serde_json::Value =
            serde_json::from_str(r#"[["a\tb"]]"#).unwrap();
        assert!(unflatten_sv(&json, &SvOpts::tsv()).is_err());
    }
}
