//! Environment file parser (.env format)
//!
//! Parses KEY=VALUE lines with support for:
//! - Comments (#) and inline comments
//! - Single-quoted values (no expansion)
//! - Double-quoted values (with variable expansion)
//! - Unquoted values (with variable expansion)
//! - ${VAR} and $VAR expansion from current environment

/// Parse a .env file into key-value pairs.
/// `current_env` is used for ${VAR} expansion within values.
pub fn parse_env_file(
    content: &str,
    current_env: &std::collections::HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut accumulated = current_env.clone();

    for raw_line in content.lines() {
        let line = raw_line.trim();

        // Skip blank lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Optional "export " prefix
        let line = if line.starts_with("export ") {
            line[7..].trim_start()
        } else {
            line
        };

        // Find '='
        let eq_pos = match line.find('=') {
            Some(pos) => pos,
            None => continue, // Malformed line
        };

        let key = line[..eq_pos].trim_end();
        if key.is_empty() {
            continue;
        }

        // Validate key: alphanumeric + underscore
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }

        let raw_value = &line[eq_pos + 1..];
        let value = unquote_value(raw_value, &accumulated);

        result.push((key.to_string(), value.clone()));
        accumulated.insert(key.to_string(), value);
    }

    result
}

fn strip_inline_comment(s: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = s.chars().collect();

    for i in 0..chars.len() {
        let ch = chars[i];
        if ch == '\'' && !in_double {
            in_single = !in_single;
        } else if ch == '"' && !in_single {
            in_double = !in_double;
        } else if ch == '#' && !in_single && !in_double {
            if i > 0 && (chars[i - 1] == ' ' || chars[i - 1] == '\t') {
                return s[..i].trim_end();
            }
        }
    }
    s
}

fn expand_vars(s: &str, env: &std::collections::HashMap<String, String>) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '$' {
            result.push(chars[i]);
            i += 1;
            continue;
        }

        i += 1; // skip '$'
        if i >= chars.len() {
            result.push('$');
            break;
        }

        if chars[i] == '{' {
            // ${VAR} form
            i += 1; // skip '{'
            let start = i;
            while i < chars.len() && chars[i] != '}' {
                i += 1;
            }
            let var_name: String = chars[start..i].iter().collect();
            if i < chars.len() {
                i += 1; // skip '}'
            }
            if let Some(val) = env.get(&var_name) {
                result.push_str(val);
            }
        } else if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            // $VAR form
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_')
            {
                i += 1;
            }
            let var_name: String = chars[start..i].iter().collect();
            if let Some(val) = env.get(&var_name) {
                result.push_str(val);
            }
        } else {
            result.push('$');
            // Don't advance i; let next iteration handle it
        }
    }

    result
}

fn unquote_value(raw: &str, env: &std::collections::HashMap<String, String>) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let bytes = raw.as_bytes();

    // Single-quoted: literal (no expansion, no escapes)
    if bytes[0] == b'\'' && *bytes.last().unwrap() == b'\'' && bytes.len() >= 2 {
        return raw[1..raw.len() - 1].to_string();
    }

    // Double-quoted: expansion but no shell escapes beyond \"
    if bytes[0] == b'"' && *bytes.last().unwrap() == b'"' && bytes.len() >= 2 {
        let inner = &raw[1..raw.len() - 1];
        // Unescape \" inside double quotes
        let mut unescaped = String::with_capacity(inner.len());
        let inner_chars: Vec<char> = inner.chars().collect();
        let mut i = 0;
        while i < inner_chars.len() {
            if inner_chars[i] == '\\' && i + 1 < inner_chars.len() && inner_chars[i + 1] == '"' {
                unescaped.push('"');
                i += 2;
            } else {
                unescaped.push(inner_chars[i]);
                i += 1;
            }
        }
        return expand_vars(&unescaped, env);
    }

    // Unquoted: expand, strip inline comment, trim
    let stripped = strip_inline_comment(raw);
    let trimmed = stripped.trim_end();
    expand_vars(trimmed, env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let content = "FOO=bar\nBAZ=qux\n";
        let env = std::collections::HashMap::new();
        let result = parse_env_file(content, &env);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("FOO".to_string(), "bar".to_string()));
        assert_eq!(result[1], ("BAZ".to_string(), "qux".to_string()));
    }

    #[test]
    fn test_parse_comments_and_blanks() {
        let content = "# comment\n\nFOO=bar\n# another\n";
        let env = std::collections::HashMap::new();
        let result = parse_env_file(content, &env);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "FOO");
    }

    #[test]
    fn test_parse_export_prefix() {
        let content = "export FOO=bar\n";
        let env = std::collections::HashMap::new();
        let result = parse_env_file(content, &env);
        assert_eq!(result[0], ("FOO".to_string(), "bar".to_string()));
    }

    #[test]
    fn test_parse_single_quoted() {
        let content = "FOO='hello world'\n";
        let env = std::collections::HashMap::new();
        let result = parse_env_file(content, &env);
        assert_eq!(result[0].1, "hello world");
    }

    #[test]
    fn test_parse_double_quoted_with_expansion() {
        let mut env = std::collections::HashMap::new();
        env.insert("BASE".to_string(), "/opt".to_string());
        let content = "PATH=\"${BASE}/bin\"\n";
        let result = parse_env_file(content, &env);
        assert_eq!(result[0].1, "/opt/bin");
    }

    #[test]
    fn test_parse_dollar_var_expansion() {
        let mut env = std::collections::HashMap::new();
        env.insert("HOME".to_string(), "/home/user".to_string());
        let content = "CONFIG=$HOME/.config\n";
        let result = parse_env_file(content, &env);
        assert_eq!(result[0].1, "/home/user/.config");
    }

    #[test]
    fn test_parse_accumulated_env() {
        let env = std::collections::HashMap::new();
        let content = "BASE=/opt\nPATH=${BASE}/bin\n";
        let result = parse_env_file(content, &env);
        assert_eq!(result[1].1, "/opt/bin");
    }

    #[test]
    fn test_parse_invalid_key_skipped() {
        let content = "IN-VALID=value\nVALID=ok\n";
        let env = std::collections::HashMap::new();
        let result = parse_env_file(content, &env);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "VALID");
    }

    #[test]
    fn test_parse_inline_comment() {
        let content = "FOO=bar # this is a comment\n";
        let env = std::collections::HashMap::new();
        let result = parse_env_file(content, &env);
        assert_eq!(result[0].1, "bar");
    }

    #[test]
    fn test_parse_double_quote_escape() {
        let content = "MSG=\"say \\\"hello\\\"\"\n";
        let env = std::collections::HashMap::new();
        let result = parse_env_file(content, &env);
        assert_eq!(result[0].1, "say \"hello\"");
    }
}
