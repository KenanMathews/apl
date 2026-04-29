/// APL command parser
///
/// Takes a raw APL string like:
///   AGENT.FS.read("workspace/notes.txt")
///   AGENT.SYS.run("python3 analyse.py", timeout=60)
///   SESSION.notify("Done", "Report ready.")
///
/// And produces a structured Command.

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub namespace: String,      // AGENT or SESSION
    pub subspace: String,       // FS, SYS, MEM, NET, PROC, LOG, etc.
    pub action: String,         // read, write, run, notify, etc.
    pub args: Vec<Arg>,         // all arguments in order
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Positional(Value),
    Named(String, Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    List(Vec<Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_str_owned(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<Value>> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }
}

/// Parse error
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("empty input")]
    Empty,
    #[error("invalid command format: {0}")]
    InvalidFormat(String),
    #[error("unknown namespace: {0}")]
    UnknownNamespace(String),
    #[error("parse error at position {pos}: {msg}")]
    SyntaxError { pos: usize, msg: String },
}

/// Parse a single APL command string into a Command struct
pub fn parse(input: &str) -> Result<Command, ParseError> {
    let input = input.trim();

    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    // Split on first '(' to get name and args
    let paren_pos = input.find('(').ok_or_else(|| {
        ParseError::InvalidFormat(format!("no opening parenthesis in: {}", input))
    })?;

    let name_part = &input[..paren_pos];
    let args_part = input[paren_pos + 1..].trim_end_matches(')');

    // Parse name: NAMESPACE.SUBSPACE.action or NAMESPACE.action (for ESCALATE)
    let parts: Vec<&str> = name_part.split('.').collect();

    let (namespace, subspace, action) = match parts.len() {
        // AGENT.ESCALATE() — two parts
        2 => (
            parts[0].to_uppercase(),
            String::new(),
            parts[1].to_lowercase(),
        ),
        // AGENT.FS.read() — three parts
        3 => (
            parts[0].to_uppercase(),
            parts[1].to_uppercase(),
            parts[2].to_lowercase(),
        ),
        // SESSION.watch.stop() — four parts, join last two
        4 => (
            parts[0].to_uppercase(),
            parts[1].to_uppercase(),
            format!("{}.{}", parts[2].to_lowercase(), parts[3].to_lowercase()),
        ),
        _ => {
            return Err(ParseError::InvalidFormat(format!(
                "expected NAMESPACE.SUBSPACE.action, got: {}",
                name_part
            )))
        }
    };

    // Validate namespace
    if namespace != "AGENT" && namespace != "SESSION" {
        return Err(ParseError::UnknownNamespace(namespace));
    }

    // Parse arguments
    let args = parse_args(args_part)?;

    Ok(Command {
        namespace,
        subspace,
        action,
        args,
    })
}

/// Parse argument list: "arg1, arg2, named=value, ..."
fn parse_args(input: &str) -> Result<Vec<Arg>, ParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(vec![]);
    }

    let mut args = Vec::new();
    let tokens = split_args(input)?;

    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        // Check if named: key=value
        if let Some(eq_pos) = find_equals(token) {
            let key = token[..eq_pos].trim().to_string();
            let val_str = token[eq_pos + 1..].trim();
            let value = parse_value(val_str)?;
            args.push(Arg::Named(key, value));
        } else {
            let value = parse_value(token)?;
            args.push(Arg::Positional(value));
        }
    }

    Ok(args)
}

/// Find the position of '=' that is not inside quotes or brackets
fn find_equals(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;

    for (i, ch) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_str => escape = true,
            '"' => in_str = !in_str,
            '[' if !in_str => depth += 1,
            ']' if !in_str => depth -= 1,
            '=' if !in_str && depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Split args on commas, respecting strings and nested brackets
fn split_args(input: &str) -> Result<Vec<String>, ParseError> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_str => {
                current.push(ch);
                escape = true;
            }
            '"' => {
                in_str = !in_str;
                current.push(ch);
            }
            '[' if !in_str => {
                depth += 1;
                current.push(ch);
            }
            ']' if !in_str => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_str && depth == 0 => {
                result.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    Ok(result)
}

/// Parse a single value token
fn parse_value(s: &str) -> Result<Value, ParseError> {
    let s = s.trim();

    // null
    if s == "null" {
        return Ok(Value::Null);
    }

    // bool
    if s == "true" {
        return Ok(Value::Bool(true));
    }
    if s == "false" {
        return Ok(Value::Bool(false));
    }

    // quoted string
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        let unescaped = inner.replace("\\\"", "\"").replace("\\n", "\n").replace("\\t", "\t");
        return Ok(Value::Str(unescaped));
    }

    // list [item, item]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let items = split_args(inner)?;
        let values: Result<Vec<Value>, _> = items
            .iter()
            .filter(|i| !i.trim().is_empty())
            .map(|i| parse_value(i))
            .collect();
        return Ok(Value::List(values?));
    }

    // integer
    if let Ok(i) = s.parse::<i64>() {
        return Ok(Value::Int(i));
    }

    // float
    if let Ok(f) = s.parse::<f64>() {
        return Ok(Value::Float(f));
    }

    // bare word (treat as string)
    Ok(Value::Str(s.to_string()))
}

/// Helper: get a positional arg by index as a string
impl Command {
    pub fn pos_str(&self, index: usize) -> Option<String> {
        let positionals: Vec<&Value> = self.args.iter()
            .filter_map(|a| if let Arg::Positional(v) = a { Some(v) } else { None })
            .collect();
        positionals.get(index).and_then(|v| v.as_str_owned())
    }

    pub fn pos_int(&self, index: usize) -> Option<i64> {
        let positionals: Vec<&Value> = self.args.iter()
            .filter_map(|a| if let Arg::Positional(v) = a { Some(v) } else { None })
            .collect();
        positionals.get(index).and_then(|v| v.as_int())
    }

    /// Get a named arg as string, with optional default
    pub fn named_str(&self, key: &str) -> Option<String> {
        self.args.iter().find_map(|a| {
            if let Arg::Named(k, v) = a {
                if k == key { v.as_str_owned() } else { None }
            } else {
                None
            }
        })
    }

    pub fn named_str_default<'a>(&'a self, key: &str, default: &'a str) -> String {
        self.named_str(key).unwrap_or_else(|| default.to_string())
    }

    pub fn named_int(&self, key: &str) -> Option<i64> {
        self.args.iter().find_map(|a| {
            if let Arg::Named(k, v) = a {
                if k == key { v.as_int() } else { None }
            } else {
                None
            }
        })
    }

    pub fn named_int_default(&self, key: &str, default: i64) -> i64 {
        self.named_int(key).unwrap_or(default)
    }

    pub fn named_bool(&self, key: &str) -> Option<bool> {
        self.args.iter().find_map(|a| {
            if let Arg::Named(k, v) = a {
                if k == key { v.as_bool() } else { None }
            } else {
                None
            }
        })
    }

    pub fn named_bool_default(&self, key: &str, default: bool) -> bool {
        self.named_bool(key).unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_read() {
        let cmd = parse(r#"AGENT.FS.read("workspace/notes.txt")"#).unwrap();
        assert_eq!(cmd.namespace, "AGENT");
        assert_eq!(cmd.subspace, "FS");
        assert_eq!(cmd.action, "read");
        assert_eq!(cmd.pos_str(0), Some("workspace/notes.txt".to_string()));
    }

    #[test]
    fn test_parse_named_args() {
        let cmd = parse(r#"AGENT.FS.write("outbox/out.txt", "hello", append=true)"#).unwrap();
        assert_eq!(cmd.pos_str(0), Some("outbox/out.txt".to_string()));
        assert_eq!(cmd.pos_str(1), Some("hello".to_string()));
        assert_eq!(cmd.named_bool("append"), Some(true));
    }

    #[test]
    fn test_parse_session_notify() {
        let cmd = parse(r#"SESSION.notify("Done", "Report ready.", urgency="critical")"#).unwrap();
        assert_eq!(cmd.namespace, "SESSION");
        assert_eq!(cmd.action, "notify");
        assert_eq!(cmd.pos_str(0), Some("Done".to_string()));
        assert_eq!(cmd.named_str("urgency"), Some("critical".to_string()));
    }

    #[test]
    fn test_parse_no_args() {
        let cmd = parse("AGENT.MEM.list()").unwrap();
        assert_eq!(cmd.action, "list");
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn test_parse_int_arg() {
        let cmd = parse("AGENT.PROC.kill(4821)").unwrap();
        assert_eq!(cmd.pos_int(0), Some(4821));
    }

    #[test]
    fn test_parse_list_arg() {
        let cmd = parse(r#"SESSION.screenshot(region=[0, 0, 1920, 1080])"#).unwrap();
        let region = cmd.args.iter().find_map(|a| {
            if let Arg::Named(k, Value::List(v)) = a {
                if k == "region" { Some(v) } else { None }
            } else { None }
        });
        assert!(region.is_some());
        assert_eq!(region.unwrap().len(), 4);
    }

    #[test]
    fn test_invalid_namespace() {
        let result = parse(r#"BADNS.FS.read("file")"#);
        assert!(matches!(result, Err(ParseError::UnknownNamespace(_))));
    }

    #[test]
    fn test_escalate_two_part() {
        let cmd = parse(r#"AGENT.ESCALATE("I need help")"#).unwrap();
        assert_eq!(cmd.namespace, "AGENT");
        assert_eq!(cmd.action, "escalate");
        assert_eq!(cmd.pos_str(0), Some("I need help".to_string()));
    }
}
