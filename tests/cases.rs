//! Gates for C1, C2, C4–C7, C9–C13 and C18: the compile-time checks.
//!
//! - Every `tests/reject/src/bin/<case>.rs` fails `cargo check` at the line marked
//!   `// ~ERROR`, with the code and message fragment the marker states, and nowhere else.
//! - Every case's twin (`--cfg twin`) compiles, so no case fails for an unrelated reason.
//! - Every `tests/accept/src/bin/<case>.rs` compiles and runs to completion.
//! - rustdoc checks `compile_fail` error codes: the wrong-code canary in
//!   `tests/reject/src/lib.rs` must fail.
//!
//! Each check runs cargo on the detached package in its own target directory under
//! `target/gates`, so it never waits on the build lock of the `cargo test` running it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cargo(package: &str, target: &str) -> Command {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()));
    cmd.current_dir(root().join("tests").join(package))
        .arg("--color=never")
        .env("CARGO_TARGET_DIR", root().join("target").join("gates").join(target))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTDOCFLAGS");
    cmd
}

fn run(cmd: &mut Command) -> Output {
    cmd.output().unwrap_or_else(|e| panic!("can't run {cmd:?}: {e}"))
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

// ------------------------------------------------------------------ the cases

/// A reject case and what its marker line says about the error it expects.
struct Case {
    name: String,
    line: usize,
    code: Option<String>,
    fragment: Option<String>,
    has_twin: bool,
}

fn cases() -> Vec<Case> {
    let dir = root().join("tests/reject/src/bin");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no reject cases in {}", dir.display());
    paths.iter().map(|path| case(path)).collect()
}

fn case(path: &Path) -> Case {
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
    let source = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let markers: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| line.split_once("// ~ERROR").map(|(_, rest)| (i + 1, rest.trim())))
        .collect();
    let [(line, rest)] = markers.as_slice() else {
        panic!("{name}: needs exactly one `// ~ERROR` marker, found {}", markers.len())
    };
    let (code, fragment) = match rest.split_once('"') {
        Some((code, quoted)) => (code.trim(), Some(quoted.trim_end_matches('"').to_owned())),
        None => (*rest, None),
    };
    let code = (!code.is_empty()).then(|| code.to_owned());
    let has_twin = source.lines().any(|l| l.trim() == "#[cfg(twin)]");
    Case { name, line: *line, code, fragment, has_twin }
}

/// One error rustc reported, reduced to what the gates need.
#[derive(Debug)]
struct Diagnostic {
    target: String,
    code: Option<String>,
    message: String,
    /// Where the error lands in the case's own source, following macro expansions back to
    /// the call site: `(file, first line, last line)`.
    site: Option<(String, usize, usize)>,
}

/// `cargo check` with JSON messages: whether it succeeded, and every error.
fn check(package: &str, target: &str, twin: bool) -> (bool, Vec<Diagnostic>, String) {
    let mut cmd = cargo(package, target);
    cmd.args(["check", "--bins", "--keep-going", "--message-format=json"]);
    if twin {
        cmd.env("RUSTFLAGS", "--cfg twin");
    }
    let out = run(&mut cmd);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut errors = Vec::new();
    for line in stdout.lines().filter(|l| l.starts_with('{')) {
        let Some(msg) = Json::parse(line) else { continue };
        if msg.get("reason").and_then(Json::str) != Some("compiler-message") {
            continue;
        }
        let Some(message) = msg.get("message") else { continue };
        if message.get("level").and_then(Json::str) != Some("error") {
            continue;
        }
        let spans = message.get("spans").and_then(Json::array).unwrap_or(&[]);
        if spans.is_empty() {
            continue; // "aborting due to N previous errors"
        }
        let site = spans.iter().find(|s| s.get("is_primary").and_then(Json::bool) == Some(true)).map(user_site);
        errors.push(Diagnostic {
            target: msg.get("target").and_then(|t| t.get("name")).and_then(Json::str).unwrap_or_default().to_owned(),
            code: message.get("code").and_then(|c| c.get("code")).and_then(Json::str).map(str::to_owned),
            message: message.get("message").and_then(Json::str).unwrap_or_default().to_owned(),
            site,
        });
    }
    (out.status.success(), errors, text(&out))
}

/// Follows a span out of macro expansions until it lands in a file of the package being
/// checked (a relative path), and returns that file and lines.
fn user_site(span: &Json) -> (String, usize, usize) {
    let mut span = span;
    loop {
        let file = span.get("file_name").and_then(Json::str).unwrap_or_default();
        let parent = span.get("expansion").and_then(|e| e.get("span"));
        match parent {
            Some(parent) if Path::new(file).is_absolute() || file.starts_with('<') => span = parent,
            _ => {
                let first = span.get("line_start").and_then(Json::num).unwrap_or(0);
                let last = span.get("line_end").and_then(Json::num).unwrap_or(first);
                return (file.to_owned(), first, last);
            }
        }
    }
}

#[test]
fn every_reject_case_fails_at_its_marker() {
    let cases = cases();
    let (ok, errors, output) = check("reject", "reject", false);
    assert!(!ok, "every reject case compiled:\n{output}");
    let mut by_case: BTreeMap<&str, Vec<&Diagnostic>> = BTreeMap::new();
    for error in &errors {
        by_case.entry(error.target.as_str()).or_default().push(error);
    }
    let mut failures = Vec::new();
    for case in &cases {
        let file = format!("src/bin/{}.rs", case.name);
        let errors = by_case.get(case.name.as_str()).map_or(&[][..], Vec::as_slice);
        if errors.is_empty() {
            failures.push(format!("{}: compiles, but must fail at line {}", case.name, case.line));
            continue;
        }
        for error in errors {
            match &error.site {
                Some((f, first, last)) if *f == file && (*first..=*last).contains(&case.line) => {}
                site => failures.push(format!(
                    "{}: fails at {site:?}, not only at its marker (line {}): {}",
                    case.name, case.line, error.message
                )),
            }
        }
        let expected = errors.iter().any(|e| {
            case.code.as_ref().is_none_or(|code| e.code.as_ref() == Some(code))
                && case.fragment.as_ref().is_none_or(|fragment| e.message.contains(fragment.as_str()))
        });
        if !expected {
            let got: Vec<_> = errors.iter().map(|e| format!("{:?} {}", e.code, e.message)).collect();
            failures.push(format!(
                "{}: expected {:?} {:?} at line {}, got {got:?}",
                case.name, case.code, case.fragment, case.line
            ));
        }
    }
    let unknown: Vec<_> = by_case.keys().filter(|t| !cases.iter().any(|c| c.name == **t)).collect();
    assert!(unknown.is_empty(), "errors in targets that aren't cases: {unknown:?}\n{output}");
    assert!(failures.is_empty(), "{} of {} reject cases are wrong:\n{}", failures.len(), cases.len(), failures.join("\n"));
}

#[test]
fn every_twin_compiles() {
    let missing: Vec<String> = cases().into_iter().filter(|c| !c.has_twin).map(|c| c.name).collect();
    assert!(missing.is_empty(), "cases without a `#[cfg(twin)]` twin: {missing:?}");
    let (ok, errors, output) = check("reject", "reject-twin", true);
    assert!(ok && errors.is_empty(), "twins must compile:\n{output}");
}

#[test]
fn every_accept_case_compiles() {
    let out = run(cargo("accept", "accept").args(["build", "--bins"]));
    assert!(out.status.success(), "accept cases must compile:\n{}", text(&out));
    let dir = root().join("tests/accept/src/bin");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter_map(|e| e.path().file_stem().and_then(|s| s.to_str()).map(str::to_owned))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no accept cases");
    for name in names {
        let bin = root().join("target/gates/accept/debug").join(&name);
        let out = run(&mut Command::new(&bin));
        assert!(out.status.success(), "accept case {name} failed when run:\n{}", text(&out));
    }
}

#[test]
fn wrong_code_canary_fails() {
    let out = run(cargo("reject", "canary").args(["test", "--doc"]));
    let output = text(&out);
    assert!(!out.status.success(), "the wrong-code canary passed, so rustdoc isn't checking error codes:\n{output}");
    assert!(
        output.contains("E0308") && output.contains("not found"),
        "the canary must fail because its code is wrong, not for another reason:\n{output}"
    );
}

// ------------------------------------------------------------------ a small JSON reader

/// Just enough JSON for cargo's messages.
#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn parse(text: &str) -> Option<Json> {
        let mut p = Parser { bytes: text.as_bytes(), at: 0 };
        let value = p.value()?;
        p.ws();
        (p.at == p.bytes.len()).then_some(value)
    }
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    fn bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
    fn num(&self) -> Option<usize> {
        match self {
            Json::Num(n) if *n >= 0.0 => Some(*n as usize),
            _ => None,
        }
    }
    fn array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items),
            Json::Null => Some(&[]),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1;
        }
    }
    fn eat(&mut self, word: &str) -> Option<()> {
        self.bytes.get(self.at..self.at + word.len()).filter(|b| *b == word.as_bytes())?;
        self.at += word.len();
        Some(())
    }
    fn value(&mut self) -> Option<Json> {
        self.ws();
        match self.peek()? {
            b'n' => self.eat("null").map(|()| Json::Null),
            b't' => self.eat("true").map(|()| Json::Bool(true)),
            b'f' => self.eat("false").map(|()| Json::Bool(false)),
            b'"' => self.string().map(Json::Str),
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.ws();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Some(Json::Arr(items));
                }
                loop {
                    items.push(self.value()?);
                    self.ws();
                    match self.peek()? {
                        b',' => self.at += 1,
                        b']' => {
                            self.at += 1;
                            return Some(Json::Arr(items));
                        }
                        _ => return None,
                    }
                }
            }
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.ws();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Some(Json::Obj(fields));
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    self.eat(":")?;
                    fields.push((key, self.value()?));
                    self.ws();
                    match self.peek()? {
                        b',' => self.at += 1,
                        b'}' => {
                            self.at += 1;
                            return Some(Json::Obj(fields));
                        }
                        _ => return None,
                    }
                }
            }
            _ => {
                let start = self.at;
                while matches!(self.peek(), Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.at += 1;
                }
                std::str::from_utf8(self.bytes.get(start..self.at)?).ok()?.parse().ok().map(Json::Num)
            }
        }
    }
    fn string(&mut self) -> Option<String> {
        self.eat("\"")?;
        let mut out = Vec::new();
        loop {
            let b = self.peek()?;
            self.at += 1;
            match b {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = self.peek()?;
                    self.at += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = std::str::from_utf8(self.bytes.get(self.at..self.at + 4)?).ok()?;
                            self.at += 4;
                            let mut code = u32::from_str_radix(hex, 16).ok()?;
                            if (0xD800..0xDC00).contains(&code) {
                                self.eat("\\u")?;
                                let low = std::str::from_utf8(self.bytes.get(self.at..self.at + 4)?).ok()?;
                                self.at += 4;
                                let low = u32::from_str_radix(low, 16).ok()?;
                                code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                            }
                            let c = char::from_u32(code).unwrap_or('\u{fffd}');
                            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }
}
