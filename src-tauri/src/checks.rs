//! Deterministic static checks on the files PastePilot read. Pure text
//! analysis, nothing is executed. Results are verified facts that go to the
//! AI (and the UI) alongside the code:
//!
//! - imports that don't resolve, or only resolve with different casing
//!   (works on macOS/Windows, breaks on Linux hosts like Netlify/Vercel)
//! - default/named imports the target file doesn't export
//! - packages imported but missing from package.json
//! - React/JSX mistakes: class=, for=, lowercase event handlers, string styles,
//!   lowercase component names
//! - HTML tags that are never closed or closed in the wrong order
//! - CSS with unbalanced braces
//! - package.json that isn't valid JSON

use crate::repo_search::resolve_import;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const MAX_CHECKS: usize = 15;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub file: String,
    pub line: Option<u32>,
    /// import | casing | export | dependency | jsx | html | css | json
    pub kind: String,
    pub message: String,
}

fn check(file: &str, line: usize, kind: &str, message: String) -> Check {
    Check { file: file.to_string(), line: (line > 0).then_some(line as u32), kind: kind.to_string(), message }
}

fn ext(path: &str) -> String {
    path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

fn is_script(path: &str) -> bool {
    matches!(ext(path).as_str(), "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "vue" | "svelte")
}

fn line_of(content: &str, byte: usize) -> usize {
    content[..byte.min(content.len())].matches('\n').count() + 1
}

pub fn run(files: &[(String, Arc<str>)], all_paths: &HashSet<String>, package_json: Option<&Value>) -> Vec<Check> {
    let mut out = Vec::new();
    let lower_paths: HashMap<String, String> = all_paths.iter().map(|p| (p.to_lowercase(), p.clone())).collect();
    let lower_set: HashSet<String> = lower_paths.keys().cloned().collect();
    let contents: HashMap<&str, &str> = files.iter().map(|(p, c)| (p.as_str(), c.as_ref())).collect();
    let deps: Option<HashSet<String>> = package_json.map(|p| crate::project::dependencies(p).into_iter().collect());
    let top_dirs: HashSet<String> = all_paths
        .iter()
        .flat_map(|p| p.split('/').take(2).map(|s| s.to_lowercase()).collect::<Vec<_>>())
        .collect();

    for (path, content) in files {
        let e = ext(path);
        if is_script(path) {
            for imp in imports(content) {
                let local = imp.spec.starts_with('.') || imp.spec.starts_with("@/") || imp.spec.starts_with("~/") || imp.spec.starts_with('/');
                if local {
                    match resolve_import(path, &imp.spec, all_paths) {
                        Some(target) => {
                            if let Some(target_content) = contents.get(target.as_str()) {
                                out.extend(export_problems(path, &imp, &target, target_content));
                            }
                        }
                        None => {
                            let ci = resolve_import(&path.to_lowercase(), &imp.spec.to_lowercase(), &lower_set)
                                .and_then(|l| lower_paths.get(&l).cloned());
                            out.push(match ci {
                                Some(actual) => check(path, imp.line, "casing", format!(
                                    "Imports \"{}\" but the file is \"{actual}\". The letter casing differs: this works on macOS/Windows but breaks on Linux hosts (Netlify, Vercel, GitHub Pages).",
                                    imp.spec
                                )),
                                None => check(path, imp.line, "import", format!("Imports \"{}\", which doesn't match any file in the repository.", imp.spec)),
                            });
                        }
                    }
                } else if let Some(deps) = &deps {
                    let root = package_root(&imp.spec);
                    let first = root.trim_start_matches('@').to_lowercase();
                    let builtin = imp.spec.starts_with("node:") || NODE_BUILTINS.contains(&root.as_str());
                    // Skip path aliases like "components/Header" or "@components/x" that point at local folders.
                    let alias = top_dirs.contains(&first) || top_dirs.contains(first.split('/').next().unwrap_or(""));
                    if !builtin && !alias && !root.is_empty() && !deps.contains(&root) {
                        out.push(check(path, imp.line, "dependency", format!(
                            "Imports the package \"{root}\", but it isn't listed in package.json. It needs `npm install {root}`."
                        )));
                    }
                }
            }
            if matches!(e.as_str(), "jsx" | "tsx") {
                out.extend(jsx_problems(path, content));
            }
        } else if matches!(e.as_str(), "html" | "htm") {
            out.extend(html_problems(path, content));
            for spec in crate::repo_search::local_imports(path, content) {
                if resolve_import(path, &spec, all_paths).is_none() && !spec.starts_with('#') {
                    let line = content.find(&spec).map(|b| line_of(content, b)).unwrap_or(0);
                    let ci = resolve_import(&path.to_lowercase(), &spec.to_lowercase(), &lower_set).and_then(|l| lower_paths.get(&l).cloned());
                    out.push(match ci {
                        Some(actual) => check(path, line, "casing", format!("Links \"{spec}\" but the file is \"{actual}\" (casing differs; breaks on Linux hosts).")),
                        None => check(path, line, "import", format!("Links \"{spec}\", which doesn't exist in the repository.")),
                    });
                }
            }
        } else if matches!(e.as_str(), "css" | "scss" | "less") {
            out.extend(css_problems(path, content));
        } else if path.ends_with("package.json") {
            if let Err(err) = serde_json::from_str::<Value>(content) {
                out.push(check(path, err.line(), "json", format!("package.json isn't valid JSON: {err}")));
            }
        }
    }
    out.dedup();
    out.truncate(MAX_CHECKS);
    out
}

const NODE_BUILTINS: &[&str] = &[
    "fs", "path", "os", "http", "https", "url", "util", "crypto", "stream", "events", "child_process", "buffer",
    "assert", "zlib", "querystring", "net", "tls", "dns", "readline", "worker_threads", "process", "module", "timers",
];

fn package_root(spec: &str) -> String {
    let mut parts = spec.split('/');
    match parts.next() {
        Some(scope) if scope.starts_with('@') => match parts.next() {
            Some(name) => format!("{scope}/{name}"),
            None => scope.to_string(),
        },
        Some(name) => name.to_string(),
        None => String::new(),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Import {
    line: usize,
    spec: String,
    default: Option<String>,
    named: Vec<String>,
    type_only: bool,
}

/// ES imports (including multi-line ones) and require() calls.
fn imports(content: &str) -> Vec<Import> {
    let mut out = Vec::new();
    let bytes = content.as_bytes();
    let mut start = 0;
    while let Some(i) = content[start..].find("import") {
        let at = start + i;
        start = at + 6;
        let before_ok = at == 0 || matches!(bytes[at - 1], b'\n' | b' ' | b'\t' | b';' | b'}');
        let after = bytes.get(at + 6).copied().unwrap_or(b' ');
        if !before_ok || !(after.is_ascii_whitespace() || matches!(after, b'{' | b'*' | b'"' | b'\'')) {
            continue;
        }
        let window_end = (at + 600).min(content.len());
        let mut end = window_end;
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        let window = &content[at + 6..end];
        let Some((clause, spec)) = split_import(window) else { continue };
        let clause = clause.trim();
        let type_only = clause.starts_with("type ");
        let clause = clause.trim_start_matches("type ").trim();
        let (default, named) = parse_clause(clause);
        out.push(Import { line: line_of(content, at), spec, default, named, type_only });
    }
    let mut start = 0;
    while let Some(i) = content[start..].find("require(") {
        let at = start + i;
        start = at + 8;
        if let Some(spec) = quoted(&content[at + 8..]) {
            out.push(Import { line: line_of(content, at), spec, default: None, named: vec![], type_only: false });
        }
    }
    out
}

/// After the `import` keyword: ("clause", "spec") for `X from 'y'`, or ("", "y") for `'y'`.
fn split_import(window: &str) -> Option<(String, String)> {
    let t = window.trim_start();
    if t.starts_with(['"', '\'']) {
        return quoted(t).map(|s| (String::new(), s));
    }
    // Stop at the end of this statement so we never pair a clause with a later import's path.
    let stmt_end = t.find(';').unwrap_or(t.len());
    let stmt = &t[..stmt_end];
    let from = stmt.rfind(" from")?;
    let spec = quoted(&stmt[from + 5..])?;
    Some((stmt[..from].to_string(), spec))
}

fn quoted(s: &str) -> Option<String> {
    let t = s.trim_start();
    let q = t.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let rest = &t[1..];
    let end = rest.find(q)?;
    let spec = &rest[..end];
    (!spec.is_empty() && !spec.contains('\n')).then(|| spec.to_string())
}

fn parse_clause(clause: &str) -> (Option<String>, Vec<String>) {
    let mut default = None;
    let mut named = Vec::new();
    let (head, braces) = match (clause.find('{'), clause.rfind('}')) {
        (Some(a), Some(b)) if b > a => (format!("{}{}", &clause[..a], &clause[b + 1..]), Some(&clause[a + 1..b])),
        _ => (clause.to_string(), None),
    };
    for part in head.split(',') {
        let p = part.trim();
        if !p.is_empty() && !p.starts_with('*') && p.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            default = Some(p.to_string());
        }
    }
    if let Some(inner) = braces {
        for item in inner.split(',') {
            let item = item.trim().trim_start_matches("type ").trim();
            let name = item.split(" as ").next().unwrap_or("").trim();
            if !name.is_empty() && name != "default" && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                named.push(name.to_string());
            }
        }
    }
    (default, named)
}

fn export_problems(path: &str, imp: &Import, target: &str, target_content: &str) -> Vec<Check> {
    let mut out = Vec::new();
    if imp.type_only || !is_script(target) || matches!(ext(target).as_str(), "vue" | "svelte") {
        return out;
    }
    let commonjs = target_content.contains("module.exports") || target_content.contains("exports.");
    if commonjs || target_content.contains("export * from") {
        return out; // re-exports or CommonJS: too dynamic to judge
    }
    if let Some(name) = &imp.default {
        if !target_content.contains("export default") && !target_content.contains("as default") {
            let hint = if exports_name(target_content, name) {
                format!(" It does export `{name}` by name, so use `import {{ {name} }} from \"{}\"`.", imp.spec)
            } else {
                String::new()
            };
            out.push(check(path, imp.line, "export", format!(
                "Imports a default export from \"{}\", but {target} has no `export default`.{hint}",
                imp.spec
            )));
        }
    }
    for name in &imp.named {
        if !exports_name(target_content, name) {
            let hint = if target_content.contains("export default") { " (it only has a default export)" } else { "" };
            out.push(check(path, imp.line, "export", format!("Imports `{name}` from \"{}\", but {target} doesn't export `{name}`{hint}.", imp.spec)));
        }
    }
    out
}

fn exports_name(content: &str, name: &str) -> bool {
    for kw in ["const", "let", "var", "function", "async function", "class", "type", "interface", "enum", "function*"] {
        if contains_decl(content, &format!("export {kw} {name}")) {
            return true;
        }
    }
    // export { a, b as name }
    let mut start = 0;
    while let Some(i) = content[start..].find("export {") {
        let at = start + i + 8;
        if let Some(end) = content[at..].find('}') {
            let names = &content[at..at + end];
            if names.split(',').any(|n| {
                let n = n.trim();
                n == name || n.rsplit(" as ").next().map(str::trim) == Some(name)
            }) {
                return true;
            }
        }
        start = at;
    }
    false
}

fn contains_decl(content: &str, decl: &str) -> bool {
    content.match_indices(decl).any(|(i, _)| {
        let next = content[i + decl.len()..].chars().next();
        !next.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn jsx_problems(path: &str, content: &str) -> Vec<Check> {
    let mut out = Vec::new();
    let mut lowercase_components: Vec<String> = Vec::new();
    for imp in imports(content) {
        if let Some(name) = imp.default {
            if name.chars().next().is_some_and(|c| c.is_lowercase()) && (imp.spec.starts_with('.') || imp.spec.starts_with("@/")) {
                lowercase_components.push(name);
            }
        }
    }
    for (i, line) in content.lines().enumerate() {
        let n = i + 1;
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with('*') || !line.contains('<') {
            continue;
        }
        if line.contains(" class=\"") || line.contains(" class={") {
            out.push(check(path, n, "jsx", "Uses `class=` in JSX. React needs `className=`.".into()));
        }
        if line.contains(" for=\"") || line.contains(" for={") {
            out.push(check(path, n, "jsx", "Uses `for=` in JSX. React needs `htmlFor=`.".into()));
        }
        if line.contains(" style=\"") {
            out.push(check(path, n, "jsx", "Uses a string `style=\"...\"` in JSX. React needs an object: `style={{ color: \"red\" }}`.".into()));
        }
        if let Some(handler) = lowercase_handler(line) {
            out.push(check(path, n, "jsx", format!("Uses `{handler}=` in JSX. React event handlers are camelCase (for example `onClick`).")));
        }
        for name in &lowercase_components {
            if line.contains(&format!("<{name}")) && !line.contains(&format!("<{name}.")) {
                out.push(check(path, n, "jsx", format!(
                    "Renders `<{name}>`, but JSX treats lowercase tags as HTML elements. Rename the component import to start with a capital letter."
                )));
            }
        }
        if out.len() >= 6 {
            break;
        }
    }
    out
}

fn lowercase_handler(line: &str) -> Option<String> {
    for (i, _) in line.match_indices(" on") {
        let rest = &line[i + 3..];
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        if name.len() >= 4 && name.chars().all(|c| c.is_ascii_lowercase()) && rest[name.len()..].starts_with('=') {
            return Some(format!("on{name}"));
        }
    }
    None
}

const VOID_TAGS: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr", "param", "!doctype"];
/// Tags HTML lets you leave open; skipped to avoid false alarms.
const OPTIONAL_CLOSE: &[&str] = &["li", "p", "td", "th", "tr", "option", "thead", "tbody", "tfoot", "colgroup", "dt", "dd", "html", "head", "body", "optgroup", "rt", "rp"];

fn html_problems(path: &str, content: &str) -> Vec<Check> {
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    while let Some(off) = content[i..].find('<') {
        let at = i + off;
        if content[at..].starts_with("<!--") {
            i = content[at..].find("-->").map(|e| at + e + 3).unwrap_or(content.len());
            continue;
        }
        let Some(close) = content[at..].find('>') else { break };
        let tag_text = &content[at + 1..at + close];
        i = at + close + 1;
        let closing = tag_text.starts_with('/');
        let name: String = tag_text
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '!')
            .collect::<String>()
            .to_lowercase();
        if name.is_empty() || VOID_TAGS.contains(&name.as_str()) || OPTIONAL_CLOSE.contains(&name.as_str()) || tag_text.ends_with('/') {
            continue;
        }
        let line = line_of(content, at);
        if closing {
            match stack.iter().rposition(|(n, _)| *n == name) {
                Some(pos) if pos == stack.len() - 1 => {
                    stack.pop();
                }
                Some(pos) => {
                    let (open, open_line) = stack[pos + 1].clone();
                    return vec![check(path, open_line, "html", format!(
                        "<{open}> opened on line {open_line} isn't closed before </{name}> on line {line}."
                    ))];
                }
                None => {
                    return vec![check(path, line, "html", format!("</{name}> on line {line} has no matching opening <{name}>."))];
                }
            }
        } else {
            stack.push((name.clone(), line));
            if name == "script" || name == "style" {
                // Skip raw contents.
                let end_tag = format!("</{name}");
                i = content[i..].to_lowercase().find(&end_tag).map(|e| i + e).unwrap_or(content.len());
            }
        }
    }
    stack
        .first()
        .map(|(open, line)| vec![check(path, *line, "html", format!("<{open}> opened on line {line} is never closed."))])
        .unwrap_or_default()
}

fn css_problems(path: &str, content: &str) -> Vec<Check> {
    let mut depth: Vec<usize> = Vec::new();
    let mut chars = content.char_indices().peekable();
    let mut in_comment = false;
    let mut quote: Option<char> = None;
    while let Some((i, c)) = chars.next() {
        if in_comment {
            if c == '*' && chars.peek().is_some_and(|(_, n)| *n == '/') {
                chars.next();
                in_comment = false;
            }
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '/' if chars.peek().is_some_and(|(_, n)| *n == '*') => {
                chars.next();
                in_comment = true;
            }
            '"' | '\'' => quote = Some(c),
            '{' => depth.push(line_of(content, i)),
            '}' => {
                if depth.pop().is_none() {
                    let line = line_of(content, i);
                    return vec![check(path, line, "css", format!("Extra `}}` on line {line} with no matching `{{`. Rules after it may be ignored."))];
                }
            }
            _ => {}
        }
    }
    depth
        .last()
        .map(|line| vec![check(path, *line, "css", format!("The `{{` on line {line} is never closed, so the rules after it are likely ignored."))])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(files: &[(&str, &str)], extra_paths: &[&str], pkg: Option<Value>) -> Vec<Check> {
        let fs: Vec<(String, Arc<str>)> = files.iter().map(|(p, c)| (p.to_string(), Arc::from(*c))).collect();
        let mut all: HashSet<String> = files.iter().map(|(p, _)| p.to_string()).collect();
        all.extend(extra_paths.iter().map(|p| p.to_string()));
        run(&fs, &all, pkg.as_ref())
    }

    #[test]
    fn finds_broken_and_miscased_imports() {
        let checks = run_on(
            &[("src/App.jsx", "import Header from './components/header';\nimport Nope from './Nope';\n")],
            &["src/components/Header.jsx"],
            None,
        );
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0].kind, "casing");
        assert!(checks[0].message.contains("src/components/Header.jsx"));
        assert_eq!(checks[1].kind, "import");
        assert_eq!(checks[1].line, Some(2));
    }

    #[test]
    fn finds_missing_exports_including_multiline_imports() {
        let checks = run_on(
            &[
                ("src/App.jsx", "import Navbar from './Navbar';\nimport {\n  helper,\n  missing as m\n} from './utils';\n"),
                ("src/Navbar.jsx", "export function Navbar() { return null }\n"),
                ("src/utils.js", "export const helper = 1;\n"),
            ],
            &[],
            None,
        );
        let msgs: Vec<&str> = checks.iter().map(|c| c.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("no `export default`") && m.contains("import { Navbar }")));
        assert!(msgs.iter().any(|m| m.contains("doesn't export `missing`")));
        assert!(!msgs.iter().any(|m| m.contains("`helper`")));
    }

    #[test]
    fn finds_missing_dependencies_but_not_aliases() {
        let pkg = serde_json::json!({"dependencies": {"react": "18"}});
        let checks = run_on(
            &[("src/App.jsx", "import React from 'react';\nimport axios from 'axios';\nimport fs from 'fs';\nimport Btn from 'components/Btn';\n")],
            &["src/components/Btn.jsx"],
            Some(pkg),
        );
        assert_eq!(checks.len(), 1);
        assert!(checks[0].message.contains("\"axios\""));
    }

    #[test]
    fn finds_jsx_mistakes() {
        let checks = run_on(
            &[("src/Card.jsx", "import navbar from './Navbar';\nexport default () => (\n  <div class=\"card\" onclick={go}>\n    <navbar />\n  </div>\n);\n")],
            &["src/Navbar.jsx"],
            None,
        );
        let kinds: Vec<&str> = checks.iter().map(|c| c.message.as_str()).collect();
        assert!(kinds.iter().any(|m| m.contains("className")));
        assert!(kinds.iter().any(|m| m.contains("onclick")));
        assert!(kinds.iter().any(|m| m.contains("<navbar>")));
    }

    #[test]
    fn finds_html_and_css_structure_errors() {
        let html = "<!DOCTYPE html>\n<html><body>\n<div class=\"a\">\n  <section>\n</div>\n<img src=\"x.png\">\n<link href=\"style.css\">\n</body></html>";
        let checks = run_on(&[("index.html", html)], &["x.png"], None);
        assert!(checks.iter().any(|c| c.kind == "html" && c.message.contains("<section>")));
        assert!(checks.iter().any(|c| c.kind == "import" && c.message.contains("style.css")));

        let css = ".a { color: red; }\n.b { color: blue;\n.c { margin: 0; }\n";
        let checks = run_on(&[("style.css", css)], &[], None);
        assert_eq!(checks[0].line, Some(2));
        let ok = run_on(&[("ok.css", "/* { */ .a { content: \"}\"; }")], &[], None);
        assert!(ok.is_empty());
    }
}
