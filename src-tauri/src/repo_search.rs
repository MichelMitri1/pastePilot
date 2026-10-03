use crate::github::TreeEntry;
use std::collections::HashSet;

pub const MAX_FILE_BYTES: u64 = 200_000;

const TEXT_EXTS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "mjs", "cjs", "vue", "svelte", "html", "htm", "css", "scss", "sass", "less", "json",
    "md", "py", "java", "php", "rb", "go", "rs", "c", "cpp", "cs", "kt", "swift", "sql", "yml", "yaml", "toml",
    "txt", "sh", "ejs", "hbs", "astro",
];
const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "dist", "build", "out", ".next", ".nuxt", ".svelte-kit", "coverage", "vendor", ".vercel",
    ".netlify", ".cache", ".parcel-cache", ".idea", ".vscode", "__pycache__", ".venv", "venv", "target",
];
const SKIP_FILES: &[&str] = &["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb", ".ds_store"];
/// Import specifiers are tried with these extensions, in order.
const RESOLVE_EXTS: &[&str] = &["", ".js", ".jsx", ".ts", ".tsx", ".vue", ".svelte", ".mjs", ".css", ".scss", ".json"];

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "your", "all", "any", "can", "has", "have", "how", "its", "was",
    "this", "that", "with", "from", "what", "when", "where", "which", "will", "would", "could", "should", "there",
    "their", "about", "into", "also", "been", "were", "here", "some", "very", "more", "like", "want", "need", "know",
    "thanks", "thank", "please", "hello", "help", "really", "does", "doesn", "don", "isn", "didn", "still", "even",
    "only", "why", "tried", "try", "work", "working", "works", "showing", "show", "shows", "appear", "appears",
    "page", "file", "files", "code", "issue", "problem", "error", "errors", "get", "getting", "just", "now", "then",
    "component", "components", "function", "anything", "nothing", "something", "everything", "think", "after",
    "before", "because", "make", "made", "doesnt", "dont", "isnt", "cant", "won", "wont", "it's", "i'm", "fix",
];

pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn extension(path: &str) -> String {
    let base = basename(path);
    match base.rfind('.') {
        Some(i) if i > 0 => base[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

fn stem(path: &str) -> String {
    let base = basename(path);
    let s = match base.find('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    s.to_lowercase()
}

/// Readable source file worth considering.
pub fn is_candidate(e: &TreeEntry) -> bool {
    let lower = e.path.to_lowercase();
    if e.size > MAX_FILE_BYTES || SKIP_FILES.contains(&basename(&lower)) {
        return false;
    }
    if lower.split('/').any(|seg| SKIP_DIRS.contains(&seg)) {
        return false;
    }
    if lower.ends_with(".min.js") || lower.ends_with(".min.css") || lower.ends_with(".map") {
        return false;
    }
    let ext = extension(&lower);
    TEXT_EXTS.contains(&ext.as_str()) || basename(&lower) == "dockerfile" || basename(&lower).starts_with(".env.example")
}

pub fn candidates(entries: &[TreeEntry]) -> Vec<&TreeEntry> {
    entries.iter().filter(|e| is_candidate(e)).collect()
}

/// Shorter, src-ish paths win ties.
fn path_penalty(path: &str) -> i32 {
    let depth = path.matches('/').count() as i32;
    let src_bonus = if path.starts_with("src/") || path.contains("/src/") { 2 } else { 0 };
    depth - src_bonus
}

/// Resolves a file name you typed to a path in the repo.
pub fn resolve_user_file(query: &str, files: &[&TreeEntry]) -> Option<String> {
    let q = query.trim().trim_start_matches("./").trim_start_matches('/').to_lowercase();
    if q.is_empty() {
        return None;
    }
    let q_stem = stem(&q);
    let q_has_ext = !extension(&q).is_empty();
    let mut best: Option<(i32, i32, &str)> = None; // (tier, penalty, path)
    for e in files {
        let p = e.path.to_lowercase();
        let tier = if p == q {
            0
        } else if p.ends_with(&format!("/{q}")) {
            1
        } else if basename(&p) == q {
            2
        } else if !q_has_ext && stem(&p) == q_stem {
            3
        } else if stem(&p) == "index" && p.rsplit('/').nth(1) == Some(q_stem.as_str()) {
            4
        } else {
            continue;
        };
        let candidate = (tier, path_penalty(&e.path), e.path.as_str());
        if best.is_none_or(|b| (candidate.0, candidate.1) < (b.0, b.1)) {
            best = Some(candidate);
        }
    }
    best.map(|(_, _, p)| p.to_string())
}

/// Search terms from the issue: words, CamelCase parts, and joined word pairs
/// ("mobile menu" → "mobilemenu" to match MobileMenu.jsx).
pub fn keywords(text: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    let words: Vec<String> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let push = |w: String, out: &mut Vec<String>| {
        if w.len() >= 3 && !stop.contains(w.as_str()) && !w.chars().all(|c| c.is_ascii_digit()) && !out.contains(&w) {
            out.push(w);
        }
    };
    for w in &words {
        let lower = w.to_lowercase().replace(['-', '_'], "");
        // Capitalized common words are usually component names ("My About component", "Help page").
        if stop.contains(lower.as_str()) && w.chars().next().is_some_and(|c| c.is_uppercase()) && lower.len() >= 3 {
            if !out.contains(&lower) {
                out.push(lower);
            }
            continue;
        }
        push(lower, &mut out);
    }
    for pair in words.windows(2) {
        let (a, b) = (pair[0].to_lowercase(), pair[1].to_lowercase());
        if !stop.contains(a.as_str()) && !stop.contains(b.as_str()) && a.len() >= 3 && b.len() >= 3 {
            push(format!("{a}{b}"), &mut out);
        }
    }
    out.truncate(40);
    out
}

/// File/path matches for the issue keywords, best first.
pub fn search(files: &[&TreeEntry], keywords: &[String]) -> Vec<(String, i32)> {
    let mut scored: Vec<(String, i32)> = Vec::new();
    for e in files {
        let lower = e.path.to_lowercase();
        let ext = extension(&lower);
        if ext == "md" || ext == "txt" {
            continue; // docs are rarely the bug
        }
        let mut name = stem(&lower).replace(['-', '_'], "");
        if name == "index" {
            name = lower.rsplit('/').nth(1).unwrap_or("index").replace(['-', '_'], "");
        }
        let dirs: Vec<String> = lower.split('/').rev().skip(1).map(|d| d.replace(['-', '_'], "")).collect();
        let mut score = 0;
        for kw in keywords {
            if name == *kw {
                score += 10;
            } else if kw.len() >= 4 && (name.contains(kw.as_str()) || (name.len() >= 4 && kw.contains(name.as_str()))) {
                score += 4;
            }
            if dirs.iter().any(|d| d == kw) {
                score += 3;
            }
        }
        if score > 0 {
            score -= path_penalty(&e.path).clamp(0, 3);
            scored.push((e.path.clone(), score.max(1)));
        }
    }
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.len().cmp(&b.0.len())));
    scored
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Topics {
    pub styling: bool,
    pub routing: bool,
    pub dependencies: bool,
    pub build: bool,
    pub blank_page: bool,
}

pub fn topics(text: &str) -> Topics {
    let t = text.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
    Topics {
        styling: any(&[
            "css", "style", "styling", "color", "colour", "layout", "responsive", "mobile", "display", "flex", "grid",
            "center", "centre", "font", "margin", "padding", "hidden", "overlap", "align", "background", "width",
            "height", "tailwind", "media query", "hover", "animation", "navbar", "position",
        ]),
        routing: any(&["route", "router", "routing", "navigate", "navigation", "link", "404", "redirect", "url", "path"]),
        dependencies: any(&[
            "module not found", "cannot find module", "can't resolve", "failed to resolve", "import", "package",
            "dependency", "dependencies", "npm", "yarn", "install", "version", "peer",
        ]),
        build: any(&["vite", "webpack", "build", "deploy", "netlify", "vercel", "github pages", "gh-pages", "production"]),
        blank_page: any(&["blank", "white screen", "nothing shows", "nothing is showing", "empty page", "not rendering", "doesn't render"]),
    }
}

/// Entry points and config files that matter for the detected topics.
pub fn topic_files(files: &[&TreeEntry], topics: Topics) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let find = |pred: &dyn Fn(&str) -> bool| -> Option<String> {
        files
            .iter()
            .filter(|e| pred(&e.path.to_lowercase()))
            .min_by_key(|e| path_penalty(&e.path))
            .map(|e| e.path.clone())
    };
    let is_entry = |p: &str| {
        let s = stem(p);
        let ext = extension(p);
        (s == "app" || s == "main" || s == "index") && ["js", "jsx", "ts", "tsx", "vue", "svelte"].contains(&ext.as_str())
    };
    if topics.routing || topics.blank_page {
        if let Some(p) = find(&|p| stem(p) == "app" && is_entry(p)) {
            out.push(p);
        }
        if let Some(p) = find(&|p| ["routes", "router", "routing"].contains(&stem(p).as_str()) || p.contains("/routes/index")) {
            out.push(p);
        }
    }
    if topics.blank_page {
        if let Some(p) = find(&|p| (stem(p) == "main" || stem(p) == "index") && is_entry(p)) {
            out.push(p);
        }
        if let Some(p) = find(&|p| basename(p) == "index.html") {
            out.push(p);
        }
    }
    if topics.dependencies || topics.build {
        if let Some(p) = find(&|p| basename(p) == "package.json") {
            out.push(p);
        }
    }
    if topics.build {
        if let Some(p) = find(&|p| {
            let b = basename(p);
            b.starts_with("vite.config") || b.starts_with("webpack.config") || b == "netlify.toml" || b == "vercel.json"
        }) {
            out.push(p);
        }
    }
    out.dedup();
    out
}

/// Fallback when nothing matched: the usual entry points.
pub fn entry_points(files: &[&TreeEntry]) -> Vec<String> {
    let all = Topics { styling: false, routing: true, dependencies: true, build: false, blank_page: true };
    topic_files(files, all)
}

/// Local import specifiers in a file (relative paths, "@/" or "~/" aliases, root paths).
pub fn local_imports(path: &str, content: &str) -> Vec<String> {
    let ext = extension(path);
    let mut specs: Vec<String> = Vec::new();
    let patterns: &[&str] = match ext.as_str() {
        "css" | "scss" | "sass" | "less" => &["@import"],
        "html" | "htm" => &["src=", "href="],
        _ => &[" from", "import", "require(", "import("],
    };
    for pat in patterns {
        let mut start = 0;
        while let Some(i) = content[start..].find(pat) {
            let after = start + i + pat.len();
            if let Some(spec) = quoted_after(&content[after..]) {
                // In HTML/CSS any non-URL path is local ("styles.css"); in JS bare names are packages.
                let markup = !matches!(patterns[0], " from");
                let is_url = spec.contains("://") || spec.starts_with("//") || spec.starts_with('#')
                    || spec.starts_with("data:") || spec.starts_with("mailto:") || spec.starts_with("tel:");
                let local = !is_url
                    && (markup || spec.starts_with('.') || spec.starts_with("@/") || spec.starts_with("~/") || spec.starts_with('/'));
                if local && !spec.starts_with("//") && !specs.contains(&spec) {
                    specs.push(spec);
                }
            }
            start = after;
        }
    }
    specs.truncate(30);
    specs
}

/// Reads a quoted string after optional whitespace, "(" or "url(".
fn quoted_after(s: &str) -> Option<String> {
    let t = s.trim_start();
    let t = t.strip_prefix("url(").unwrap_or(t);
    let t = t.strip_prefix('(').unwrap_or(t).trim_start();
    let quote = t.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let rest = &t[1..];
    let end = rest.find(quote)?;
    let spec = &rest[..end];
    (!spec.is_empty() && !spec.contains('\n') && spec.len() < 200).then(|| spec.to_string())
}

/// Turns an import specifier into a repo path, trying the usual extensions and index files.
pub fn resolve_import(from: &str, spec: &str, all_paths: &HashSet<String>) -> Option<String> {
    let spec = spec.split(['?', '#']).next().unwrap_or(spec);
    let joined = if let Some(rest) = spec.strip_prefix("@/").or_else(|| spec.strip_prefix("~/")) {
        format!("src/{rest}")
    } else if let Some(rest) = spec.strip_prefix('/') {
        rest.to_string()
    } else {
        let dir = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        if dir.is_empty() { spec.to_string() } else { format!("{dir}/{spec}") }
    };
    let base = normalize(&joined)?;
    for ext in RESOLVE_EXTS {
        let p = format!("{base}{ext}");
        if all_paths.contains(&p) {
            return Some(p);
        }
    }
    for ext in &RESOLVE_EXTS[1..] {
        let p = format!("{base}/index{ext}");
        if all_paths.contains(&p) {
            return Some(p);
        }
    }
    // Root-relative paths in index.html (Vite) may point into public/.
    let p = format!("public/{base}");
    all_paths.contains(&p).then_some(p)
}

fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

pub fn is_stylesheet(path: &str) -> bool {
    matches!(extension(path).as_str(), "css" | "scss" | "sass" | "less")
}

/// Compact listing of the repo for the AI (only readable files).
pub fn tree_listing(files: &[&TreeEntry], max: usize) -> String {
    let mut paths: Vec<&str> = files.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    let extra = paths.len().saturating_sub(max);
    let mut out = paths.into_iter().take(max).collect::<Vec<_>>().join("\n");
    if extra > 0 {
        out.push_str(&format!("\n… and {extra} more files"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(paths: &[&str]) -> Vec<TreeEntry> {
        paths.iter().map(|p| TreeEntry { path: p.to_string(), size: 100, sha: p.to_string() }).collect()
    }

    #[test]
    fn filters_junk() {
        let e = entries(&["src/App.jsx", "node_modules/react/index.js", "package-lock.json", "dist/app.js", "logo.png", "a.min.js", "src/styles.css"]);
        let c: Vec<&str> = candidates(&e).iter().map(|e| e.path.as_str()).collect();
        assert_eq!(c, vec!["src/App.jsx", "src/styles.css"]);
    }

    #[test]
    fn resolves_typed_names() {
        let e = entries(&["src/components/Navbar.jsx", "src/components/Navbar.css", "src/styles.css", "src/About/index.tsx", "public/styles.css"]);
        let c = candidates(&e);
        assert_eq!(resolve_user_file("Navbar.jsx", &c).as_deref(), Some("src/components/Navbar.jsx"));
        assert_eq!(resolve_user_file("styles.css", &c).as_deref(), Some("src/styles.css"));
        assert_eq!(resolve_user_file("about", &c).as_deref(), Some("src/About/index.tsx"));
        assert_eq!(resolve_user_file("Footer.jsx", &c), None);
    }

    #[test]
    fn finds_components_from_issue_text() {
        let e = entries(&["src/components/About.jsx", "src/pages/About/index.jsx", "src/components/Footer.jsx", "src/components/MobileMenu.jsx", "README.md"]);
        let c = candidates(&e);
        let hits = search(&c, &keywords("My About component isn't showing"));
        let paths: Vec<&str> = hits.iter().map(|h| h.0.as_str()).collect();
        assert!(paths.contains(&"src/components/About.jsx"));
        assert!(paths.contains(&"src/pages/About/index.jsx"));
        assert!(!paths.contains(&"src/components/Footer.jsx"));
        let hits = search(&c, &keywords("the mobile menu won't open"));
        assert_eq!(hits[0].0, "src/components/MobileMenu.jsx");
    }

    #[test]
    fn follows_imports() {
        let all: HashSet<String> = ["src/components/Navbar.jsx", "src/components/MobileMenu.jsx", "src/components/Navbar.css", "src/utils/index.js", "src/main.jsx"]
            .iter().map(|s| s.to_string()).collect();
        let code = "import React from 'react';\nimport MobileMenu from \"./MobileMenu\";\nimport './Navbar.css';\nconst u = require('../utils');";
        let specs = local_imports("src/components/Navbar.jsx", code);
        assert_eq!(specs, vec!["./MobileMenu", "./Navbar.css", "../utils"]);
        let resolved: Vec<String> = specs.iter().filter_map(|s| resolve_import("src/components/Navbar.jsx", s, &all)).collect();
        assert_eq!(resolved, vec!["src/components/MobileMenu.jsx", "src/components/Navbar.css", "src/utils/index.js"]);
        assert_eq!(local_imports("index.html", "<link href=\"styles.css\"><a href=\"https://x.com\">"), vec!["styles.css"]);
        assert_eq!(local_imports("a.css", "@import 'base.css';"), vec!["base.css"]);
        let html = "<script type=\"module\" src=\"/src/main.jsx\"></script>";
        assert_eq!(resolve_import("index.html", &local_imports("index.html", html)[0], &all).as_deref(), Some("src/main.jsx"));
    }

    #[test]
    fn picks_topic_files() {
        let e = entries(&["package.json", "src/App.jsx", "src/main.jsx", "index.html", "vite.config.js"]);
        let c = candidates(&e);
        let t = topics("I get Module not found when I run npm run build");
        let files = topic_files(&c, t);
        assert!(files.contains(&"package.json".to_string()));
        assert!(files.contains(&"vite.config.js".to_string()));
        let files = topic_files(&c, topics("my page is blank"));
        assert!(files.contains(&"index.html".to_string()));
    }
}
