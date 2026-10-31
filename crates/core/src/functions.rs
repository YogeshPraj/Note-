//! Function List: a quick, regex-based outline of the definitions in a file.
//! It is deliberately simple (no parsing): good enough to jump around a file.

use fancy_regex::Regex;

#[derive(Debug, PartialEq, Clone)]
pub struct Entry {
    /// 0-based line of the definition.
    pub line: usize,
    pub name: String,
}

/// (pattern, label prefix). The first capture group is the name.
fn patterns(lexer: &str) -> Vec<(&'static str, &'static str)> {
    match lexer {
        "rust" => vec![
            (r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)", "fn "),
            (r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|union)\s+(\w+)", ""),
            (r"^\s*impl(?:<[^>]*>)?\s+([\w:<>, ]+?)\s*\{?\s*$", "impl "),
        ],
        "python" => vec![(r"^\s*(?:async\s+)?def\s+(\w+)", "def "), (r"^\s*class\s+(\w+)", "class ")],
        "ruby" => vec![(r"^\s*def\s+(?:self\.)?(\w+[?!]?)", "def "), (r"^\s*(?:class|module)\s+(\w+)", "class ")],
        "lua" => vec![(r"^\s*(?:local\s+)?function\s+([\w.:]+)", "function ")],
        "powershell" => vec![(r"(?i)^\s*function\s+([\w-]+)", "function ")],
        "phpscript" => vec![(r"^\s*(?:(?:public|private|protected|static|abstract|final)\s+)*function\s+(\w+)", "function "), (r"^\s*class\s+(\w+)", "class ")],
        "bash" => vec![(r"^\s*(?:function\s+)?(\w+)\s*\(\)\s*\{?", ""), (r"^\s*function\s+(\w+)", "function ")],
        "batch" => vec![(r"^\s*:(\w+)", ":")],
        "markdown" => vec![(r"^(#{1,6}\s+.+?)\s*#*\s*$", "")],
        "sql" => vec![(r"(?i)^\s*create\s+(?:or\s+replace\s+)?(?:function|procedure|view|table)\s+([\w.\[\]]+)", "")],
        "cpp" => vec![
            // JavaScript / TypeScript
            (r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*(\w+)", "function "),
            (r"^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?(?:\([^)]*\)|\w+)\s*=>", "const "),
            (r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?(?:class|interface)\s+(\w+)", "class "),
            // Go
            (r"^\s*func\s+(?:\([^)]*\)\s*)?(\w+)", "func "),
            // C, C++, Java, C#: a type and a name before "(" on a line that opens a body
            (r"^\s*(?:[\w:<>,\*&\[\]]+\s+)+[\*&]*((?!(?:if|for|while|switch|return|else|catch|sizeof|new|do)\b)[\w:~]+)\s*\([^;]*\)\s*(?:const\s*)?(?:override\s*)?(?:throws\s+[\w, ]+)?\s*\{?\s*$", ""),
        ],
        _ => Vec::new(),
    }
}

/// Definitions found in `text`, in file order.
pub fn extract(text: &str, lexer: &str) -> Vec<Entry> {
    let compiled: Vec<(Regex, &str)> = patterns(lexer).into_iter().filter_map(|(p, l)| Regex::new(p).ok().map(|r| (r, l))).collect();
    if compiled.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut in_fence = false;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if lexer == "markdown" {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
            }
            if in_fence {
                continue;
            }
        }
        for (re, prefix) in &compiled {
            if let Ok(Some(c)) = re.captures(line) {
                if let Some(m) = c.get(1) {
                    out.push(Entry { line: i, name: format!("{prefix}{}", m.as_str().trim()) });
                    break;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str, lexer: &str) -> Vec<String> {
        extract(text, lexer).into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn rust_items() {
        let t = "pub struct A {}\nimpl A {\n    pub async fn run(&self) {}\n    fn x() {}\n}\nenum E {}\n// fn commented\n";
        assert_eq!(names(t, "rust"), ["A", "impl A", "fn run", "fn x", "E"]);
    }

    #[test]
    fn python_and_markdown() {
        assert_eq!(names("class K:\n    def m(self):\n        pass\nasync def go(): pass", "python"), ["class K", "def m", "def go"]);
        assert_eq!(names("# Title\ntext\n```\n# not a heading\n```\n## Sub ##", "markdown"), ["# Title", "## Sub"]);
    }

    #[test]
    fn javascript_and_c() {
        let js = "export async function load() {}\nconst add = (a, b) => a + b;\nclass Box {}\nif (x) {\n}";
        assert_eq!(names(js, "cpp"), ["function load", "const add", "class Box"]);
        let c = "static int add(int a, int b) {\n    return add(1, 2);\n}\nvoid Foo::run() const\n{\n}\nwhile (x) {";
        assert_eq!(names(c, "cpp"), ["add", "Foo::run"]);
        assert_eq!(names("func (s *Server) Start() error {", "cpp"), ["func Start"]);
    }

    #[test]
    fn unknown_language_has_no_functions() {
        assert!(names("fn x() {}", "null").is_empty());
    }
}
