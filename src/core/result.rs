/// The APL return format (option D — tagged lines)
/// Every command produces an AplResult which renders as:
///
/// Success:
///   ok: key=value key=value
///   out: content
///
/// Failure:
///   er: key=value
///   err: error message
///
/// List:
///   ok: count=N
///     item1  col2  col3
///     item2  col2  col3

#[derive(Debug, Clone)]
pub struct AplResult {
    pub ok: bool,
    pub meta: Vec<(String, String)>,   // key=value pairs on status line
    pub out: Option<String>,           // primary output content
    pub err: Option<String>,           // error detail
    pub val: Option<String>,           // single returned value
    pub rows: Vec<Vec<String>>,        // for list results
}

impl AplResult {
    /// Build a simple success with no output
    pub fn ok() -> Self {
        Self {
            ok: true,
            meta: vec![],
            out: None,
            err: None,
            val: None,
            rows: vec![],
        }
    }

    /// Build a success with a single value
    pub fn ok_val(val: impl Into<String>) -> Self {
        Self {
            ok: true,
            meta: vec![],
            out: None,
            err: None,
            val: Some(val.into()),
            rows: vec![],
        }
    }

    /// Build a success with output content
    pub fn ok_out(out: impl Into<String>) -> Self {
        Self {
            ok: true,
            meta: vec![],
            out: Some(out.into()),
            err: None,
            val: None,
            rows: vec![],
        }
    }

    /// Build a success with metadata key=value pairs
    pub fn ok_meta(meta: Vec<(&str, String)>) -> Self {
        Self {
            ok: true,
            meta: meta.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            out: None,
            err: None,
            val: None,
            rows: vec![],
        }
    }

    /// Build a list result
    pub fn ok_list(rows: Vec<Vec<String>>) -> Self {
        let count = rows.len().to_string();
        Self {
            ok: true,
            meta: vec![("count".to_string(), count)],
            out: None,
            err: None,
            val: None,
            rows,
        }
    }

    /// Build a failure
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            meta: vec![],
            out: None,
            err: Some(msg.into()),
            val: None,
            rows: vec![],
        }
    }

    /// Build a failure with metadata (exit code, timing etc)
    pub fn err_meta(meta: Vec<(&str, String)>, msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            meta: meta.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            out: None,
            err: Some(msg.into()),
            val: None,
            rows: vec![],
        }
    }

    /// Add a metadata key=value pair
    pub fn with_meta(mut self, key: &str, val: impl Into<String>) -> Self {
        self.meta.push((key.to_string(), val.into()));
        self
    }

    /// Add output content
    pub fn with_out(mut self, out: impl Into<String>) -> Self {
        self.out = Some(out.into());
        self
    }

    /// Add a val line
    pub fn with_val(mut self, val: impl Into<String>) -> Self {
        self.val = Some(val.into());
        self
    }

    /// Render to the option D tagged-line format
    pub fn render(&self) -> String {
        let mut lines = Vec::new();

        // Status line: ok: key=val key=val  OR  er: key=val
        let meta_str = if self.meta.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = self.meta
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!(" {}", pairs.join(" "))
        };

        if self.ok {
            lines.push(format!("ok:{}", meta_str));
        } else {
            lines.push(format!("er:{}", meta_str));
        }

        // out: line
        if let Some(out) = &self.out {
            if out.contains('\n') {
                lines.push("out:".to_string());
                for line in out.lines() {
                    lines.push(format!("  {}", line));
                }
            } else {
                lines.push(format!("out: {}", out));
            }
        }

        // val: line
        if let Some(val) = &self.val {
            lines.push(format!("val: {}", val));
        }

        // err: line
        if let Some(err) = &self.err {
            lines.push(format!("err: {}", err));
        }

        // list rows — indented
        for row in &self.rows {
            lines.push(format!("  {}", row.join("  ")));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_renders() {
        let r = AplResult::ok();
        assert_eq!(r.render(), "ok:");
    }

    #[test]
    fn test_ok_meta_renders() {
        let r = AplResult::ok_meta(vec![("exit", "0".into()), ("time", "842ms".into())]);
        assert_eq!(r.render(), "ok: exit=0 time=842ms");
    }

    #[test]
    fn test_err_renders() {
        let r = AplResult::err("file not found: workspace/notes.txt");
        assert_eq!(r.render(), "er:\nerr: file not found: workspace/notes.txt");
    }

    #[test]
    fn test_list_renders() {
        let r = AplResult::ok_list(vec![
            vec!["notes.txt".into(), "file".into(), "1.0kb".into()],
            vec!["data/".into(),    "dir".into(),  "—".into()],
        ]);
        let rendered = r.render();
        assert!(rendered.starts_with("ok: count=2"));
        assert!(rendered.contains("notes.txt  file  1.0kb"));
    }

    #[test]
    fn test_multiline_out() {
        let r = AplResult::ok_out("line one\nline two\nline three");
        let rendered = r.render();
        assert!(rendered.contains("out:"));
        assert!(rendered.contains("  line one"));
        assert!(rendered.contains("  line two"));
    }
}
