//! Website inspection for Assignment Review.
//!
//! Uses the Chrome (or Edge/Brave/Chromium) installed on this Mac in headless
//! mode: it renders the page like a normal visit (JavaScript sites included),
//! returns the rendered DOM and console errors, and takes screenshots at
//! desktop, tablet and mobile widths. Each run uses a fresh, throwaway profile.
//! Without a Chromium browser it falls back to fetching the raw HTML (no
//! screenshots; visual checks become "Unable to Verify").
//!
//! The page is only viewed, never automated; no repository code is run.

use serde::Serialize;
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RENDER_BUDGET_MS: u32 = 6000;
const PROCESS_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_OUTLINE_CHARS: usize = 5000;
const MAX_LINK_CHECKS: usize = 15;

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
}

pub const VIEWPORTS: [Viewport; 3] = [
    Viewport { name: "desktop", width: 1440, height: 2000 },
    Viewport { name: "tablet", width: 820, height: 2000 },
    Viewport { name: "mobile", width: 390, height: 2200 },
];

pub fn viewports(names: &[String]) -> Vec<Viewport> {
    let v: Vec<Viewport> = VIEWPORTS.iter().copied().filter(|vp| names.iter().any(|n| n == vp.name)).collect();
    if v.is_empty() { VIEWPORTS.to_vec() } else { v }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    pub viewport: String,
    pub width: u32,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageReport {
    pub url: String,
    pub ok: bool,
    /// Rendered with a real browser (JavaScript ran).
    pub rendered: bool,
    pub title: String,
    pub console_errors: Vec<String>,
    pub broken_links: Vec<String>,
    pub shots: Vec<Shot>,
    pub error: Option<String>,
    /// Compact text description of the page for the AI (not sent to the UI).
    #[serde(skip)]
    pub outline: String,
}

/// Only http(s) URLs; adds https:// when the scheme is missing.
pub fn normalize_url(input: &str) -> Result<String, String> {
    let t = input.trim();
    if t.is_empty() {
        return Err("Missing URL.".into());
    }
    let url = if t.contains("://") { t.to_string() } else { format!("https://{t}") };
    let parsed = reqwest::Url::parse(&url).map_err(|_| format!("\"{t}\" isn't a website URL."))?;
    let host_ok = parsed.host_str().is_some_and(|h| h.contains('.') || h == "localhost");
    if !matches!(parsed.scheme(), "http" | "https") || !host_ok {
        return Err(format!("\"{t}\" isn't a website URL."));
    }
    Ok(parsed.to_string())
}

#[cfg(target_os = "macos")]
pub fn find_chrome() -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let names = [
        "Google Chrome.app/Contents/MacOS/Google Chrome",
        "Chromium.app/Contents/MacOS/Chromium",
        "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "Brave Browser.app/Contents/MacOS/Brave Browser",
        "Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
    ];
    for base in ["/Applications".to_string(), format!("{home}/Applications")] {
        for n in names {
            let p = Path::new(&base).join(n);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(windows)]
pub fn find_chrome() -> Option<PathBuf> {
    // Edge ships with Windows, so there's almost always one.
    let names = [
        r"Google\Chrome\Application\chrome.exe",
        r"Microsoft\Edge\Application\msedge.exe",
        r"BraveSoftware\Brave-Browser\Application\brave.exe",
        r"Chromium\Application\chrome.exe",
    ];
    let bases: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"].iter().filter_map(|v| std::env::var(v).ok()).collect();
    names.iter().flat_map(|n| bases.iter().map(move |b| Path::new(b).join(n))).find(|p| p.exists())
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// When a Chrome run has produced what we need. Chrome often keeps running in the
/// background after finishing (updater/helpers), so we watch the output instead of
/// waiting for it to exit.
enum Done<'a> {
    /// stdout (the DOM) has stopped growing.
    Stdout,
    /// A screenshot file exists and has stopped growing.
    File(&'a Path),
}

/// Runs headless Chrome once with a throwaway profile, stops it as soon as its
/// output is complete (or after a timeout), and cleans up every helper process.
fn run_chrome(chrome: &Path, extra: &[String], url: &str, done: Done) -> Option<(Vec<u8>, String)> {
    let profile = std::env::temp_dir().join(format!("pastepilot-chrome-{}-{}", std::process::id(), rand_suffix()));
    let mut args: Vec<String> = vec![
        "--headless=new".into(),
        "--disable-gpu".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        "--disable-sync".into(),
        "--mute-audio".into(),
        "--hide-scrollbars".into(),
        // No background work: updater, component updates, telemetry, Keychain access.
        "--disable-background-networking".into(),
        "--disable-component-update".into(),
        "--disable-default-apps".into(),
        "--disable-domain-reliability".into(),
        "--disable-client-side-phishing-detection".into(),
        "--no-service-autorun".into(),
        "--use-mock-keychain".into(),
        "--password-store=basic".into(),
        format!("--user-data-dir={}", profile.display()),
        format!("--virtual-time-budget={RENDER_BUDGET_MS}"),
    ];
    args.extend_from_slice(extra);
    args.push(url.to_string());
    let mut cmd = Command::new(chrome);
    cmd.args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    // Own process group, so we can stop Chrome *and* its helper processes.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, CREATE_NO_WINDOW);
    let mut child = cmd.spawn().ok()?;
    let pid = child.id();

    let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let err = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    for (pipe, buf) in [
        (child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), out.clone()),
        (child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>), err.clone()),
    ] {
        if let Some(mut pipe) = pipe {
            std::thread::spawn(move || {
                let mut chunk = [0u8; 16384];
                while let Ok(n) = pipe.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    buf.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&chunk[..n]);
                }
            });
        }
    }

    let start = Instant::now();
    let (mut last_size, mut stable_since) = (0u64, Instant::now());
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) || start.elapsed() > PROCESS_TIMEOUT {
            break;
        }
        let size = match done {
            Done::Stdout => out.lock().unwrap_or_else(|e| e.into_inner()).len() as u64,
            Done::File(f) => std::fs::metadata(f).map(|m| m.len()).unwrap_or(0),
        };
        if size != last_size {
            last_size = size;
            stable_since = Instant::now();
        } else if size > 0 && stable_since.elapsed() > Duration::from_millis(600) {
            break; // output complete
        }
    }
    #[cfg(unix)]
    let _ = Command::new("kill").args(["-9", &format!("-{pid}")]).stderr(Stdio::null()).status();
    #[cfg(windows)]
    let _ = std::os::windows::process::CommandExt::creation_flags(
        Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()),
        CREATE_NO_WINDOW,
    )
    .status();
    let _ = child.kill();
    let _ = child.wait();
    std::thread::sleep(Duration::from_millis(100));
    let out = std::mem::take(&mut *out.lock().unwrap_or_else(|e| e.into_inner()));
    let err = String::from_utf8_lossy(&err.lock().unwrap_or_else(|e| e.into_inner())).into_owned();
    let _ = std::fs::remove_dir_all(&profile);
    Some((out, err))
}

fn rand_suffix() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(Instant::now().elapsed().as_nanos());
    h.finish()
}

/// Rendered DOM + console errors.
fn render_dom(chrome: &Path, url: &str) -> Option<(String, Vec<String>)> {
    let (out, err) = run_chrome(
        chrome,
        &["--window-size=1440,1000".into(), "--enable-logging=stderr".into(), "--v=0".into(), "--dump-dom".into()],
        url,
        Done::Stdout,
    )?;
    let html = String::from_utf8_lossy(&out).into_owned();
    if html.trim().is_empty() {
        return None;
    }
    Some((html, console_errors(&err)))
}

/// Console messages that look like problems, from Chrome's log.
pub fn console_errors(log: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in log.lines().filter(|l| l.contains(":CONSOLE")) {
        let Some(start) = line.find("] \"") else { continue };
        let rest = &line[start + 3..];
        let msg = rest.rsplit_once("\", source:").map(|(m, _)| m).unwrap_or(rest).trim_end_matches('"');
        let lower = msg.to_lowercase();
        let problem = ["error", "uncaught", "failed", "cannot", "is not defined", "is not a function", "404", "warning:", "undefined"]
            .iter()
            .any(|k| lower.contains(k));
        let msg: String = msg.chars().take(300).collect();
        if problem && !out.contains(&msg) {
            out.push(msg);
        }
        if out.len() >= 10 {
            break;
        }
    }
    out
}

fn screenshot(chrome: &Path, url: &str, vp: Viewport) -> Option<Shot> {
    let file = std::env::temp_dir().join(format!("pastepilot-shot-{}-{}.png", vp.name, rand_suffix()));
    run_chrome(
        chrome,
        &[format!("--window-size={},{}", vp.width, vp.height), format!("--screenshot={}", file.display())],
        url,
        Done::File(&file),
    )?;
    let bytes = std::fs::read(&file).ok();
    let _ = std::fs::remove_file(&file);
    let data_url = crate::platform::image::jpeg_data_url(&bytes?, 0.72, 5_000_000)?;
    Some(Shot { viewport: vp.name.to_string(), width: vp.width, data_url })
}

/// Inspects one site: rendered DOM, console errors, screenshots per viewport, broken links.
pub async fn inspect(client: &reqwest::Client, url: &str, vps: &[Viewport], check_links: bool) -> PageReport {
    let mut report = PageReport { url: url.to_string(), ..Default::default() };
    // Reachability first: a typo'd or offline URL should say so plainly.
    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => {
            report.ok = true;
            let html = resp.text().await.unwrap_or_default();
            report.outline = outline(&html, MAX_OUTLINE_CHARS).text;
            report.title = title_of(&html);
        }
        Ok(resp) => {
            report.error = Some(format!("{url} returned HTTP {}.", resp.status().as_u16()));
            return report;
        }
        Err(_) => {
            report.error = Some(format!("Couldn't open {url}. Check the URL or that the site is deployed."));
            return report;
        }
    }

    let Some(chrome) = find_chrome() else {
        report.error = Some("Google Chrome isn't installed, so pages were read without running JavaScript and without screenshots.".into());
        return report;
    };

    // DOM and screenshots in parallel (separate throwaway browser processes).
    let dom_job = {
        let (c, u) = (chrome.clone(), url.to_string());
        tauri::async_runtime::spawn_blocking(move || render_dom(&c, &u))
    };
    let shot_jobs: Vec<_> = vps
        .iter()
        .map(|vp| {
            let (c, u, v) = (chrome.clone(), url.to_string(), *vp);
            tauri::async_runtime::spawn_blocking(move || screenshot(&c, &u, v))
        })
        .collect();

    if let Ok(Some((html, console))) = dom_job.await {
        let o = outline(&html, MAX_OUTLINE_CHARS);
        report.rendered = true;
        report.outline = o.text;
        if !o.title.is_empty() {
            report.title = o.title;
        }
        report.console_errors = console;
        if check_links {
            report.broken_links = broken_links(client, url, &html, &o.ids).await;
        }
    }
    for job in shot_jobs {
        if let Ok(Some(shot)) = job.await {
            report.shots.push(shot);
        }
    }
    report
}

pub struct Outline {
    pub text: String,
    pub title: String,
    pub ids: HashSet<String>,
}

fn title_of(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    lower
        .find("<title")
        .and_then(|i| lower[i..].find('>').map(|j| i + j + 1))
        .and_then(|start| lower[start..].find("</title>").map(|end| clean_text(&html[start..start + end])))
        .unwrap_or_default()
}

/// Strips tags and collapses whitespace.
fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out.replace("&amp;", "&").replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&#39;", "'").replace("&quot;", "\"");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let pat = format!(" {name}=");
    let i = lower.find(&pat)? + pat.len();
    let rest = &tag[i..];
    let q = rest.chars().next()?;
    if q == '"' || q == '\'' {
        rest[1..].find(q).map(|end| rest[1..1 + end].to_string())
    } else {
        Some(rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("").to_string())
    }
}

/// A compact, comparable description of a rendered page: landmarks, headings,
/// navigation, buttons, images, forms and the start of the visible text.
pub fn outline(html: &str, max_chars: usize) -> Outline {
    // Drop script/style/svg/noscript so their contents don't count as page text.
    let mut cleaned = String::with_capacity(html.len());
    let lower_all = html.to_ascii_lowercase();
    let mut i = 0;
    while i < html.len() {
        let next = ["<script", "<style", "<svg", "<noscript"].iter().filter_map(|t| lower_all[i..].find(t).map(|p| (i + p, *t))).min();
        match next {
            Some((pos, tag)) => {
                cleaned.push_str(&html[i..pos]);
                let close = format!("</{}", &tag[1..]);
                i = lower_all[pos..].find(&close).and_then(|c| lower_all[pos + c..].find('>').map(|g| pos + c + g + 1)).unwrap_or(html.len());
            }
            None => {
                cleaned.push_str(&html[i..]);
                break;
            }
        }
    }
    let lower = cleaned.to_ascii_lowercase();
    let title = title_of(html);
    let mut landmarks = Vec::new();
    let mut headings = Vec::new();
    let mut links = Vec::new();
    let mut buttons = Vec::new();
    let mut images = Vec::new();
    let (mut missing_alt, mut forms, mut inputs) = (0, 0, 0);
    let mut ids = HashSet::new();

    let mut pos = 0;
    while let Some(off) = lower[pos..].find('<') {
        let at = pos + off;
        let Some(end) = lower[at..].find('>').map(|e| at + e) else { break };
        let tag = &cleaned[at..=end];
        let name: String = lower[at + 1..end].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        pos = end + 1;
        if let Some(id) = attr(tag, "id") {
            ids.insert(id);
        }
        // Old-style anchor targets (<a name="x">) are valid "#x" destinations too.
        if name == "a" {
            if let Some(n) = attr(tag, "name") {
                ids.insert(n);
            }
        }
        let inner = |close: &str| -> String {
            lower[pos..].find(close).map(|e| clean_text(&cleaned[pos..pos + e])).unwrap_or_default()
        };
        match name.as_str() {
            "header" | "nav" | "main" | "section" | "article" | "aside" | "footer" => {
                let id = attr(tag, "id").map(|v| format!("#{v}")).unwrap_or_default();
                let class = attr(tag, "class")
                    .map(|v| v.split_whitespace().take(2).map(|c| format!(".{c}")).collect::<String>())
                    .unwrap_or_default();
                let heading = ["<h1", "<h2", "<h3"]
                    .iter()
                    .filter_map(|h| lower[pos..].find(h).map(|p| pos + p))
                    .min()
                    .filter(|p| *p < pos + 3000)
                    .and_then(|p| lower[p..].find('>').map(|g| p + g + 1))
                    .and_then(|s| lower[s..].find("</h").map(|e| clean_text(&cleaned[s..s + e])))
                    .map(|t| format!(" \"{}\"", t.chars().take(60).collect::<String>()))
                    .unwrap_or_default();
                if landmarks.len() < 30 {
                    landmarks.push(format!("{name}{id}{class}{heading}"));
                }
            }
            "h1" | "h2" | "h3" | "h4" if headings.len() < 40 => {
                let t = inner(&format!("</{name}"));
                if !t.is_empty() {
                    headings.push(format!("{} \"{}\"", name.to_uppercase(), t.chars().take(80).collect::<String>()));
                }
            }
            "a" if links.len() < 40 => {
                let text = inner("</a");
                let href = attr(tag, "href").unwrap_or_default();
                let label = if text.is_empty() { attr(tag, "aria-label").unwrap_or_else(|| "(no text)".into()) } else { text };
                links.push(format!("{}({})", label.chars().take(40).collect::<String>(), href.chars().take(80).collect::<String>()));
            }
            "button" if buttons.len() < 20 => {
                let t = inner("</button");
                buttons.push(format!("\"{}\"", if t.is_empty() { "(no text)".into() } else { t.chars().take(40).collect::<String>() }));
            }
            "input" => {
                inputs += 1;
                let ty = attr(tag, "type").unwrap_or_default().to_lowercase();
                if (ty == "submit" || ty == "button") && buttons.len() < 20 {
                    buttons.push(format!("\"{}\"", attr(tag, "value").unwrap_or_else(|| ty.clone())));
                }
            }
            "textarea" | "select" => inputs += 1,
            "form" => forms += 1,
            "img" => {
                let src = attr(tag, "src").unwrap_or_default();
                let alt = attr(tag, "alt");
                if alt.as_deref().unwrap_or("").is_empty() {
                    missing_alt += 1;
                }
                if images.len() < 20 {
                    let file = src.rsplit('/').next().unwrap_or("").split('?').next().unwrap_or("").to_string();
                    images.push(if src.is_empty() { "(empty src)".to_string() } else { file });
                }
            }
            _ => {}
        }
    }

    let body_start = lower.find("<body").unwrap_or(0);
    let text = clean_text(&cleaned[body_start..]);
    let mut out = String::new();
    out.push_str(&format!("Title: {title}\n"));
    out.push_str(&format!("Structure: {}\n", if landmarks.is_empty() { "(no header/nav/section/footer elements)".into() } else { landmarks.join(" | ") }));
    out.push_str(&format!("Headings: {}\n", headings.join(" · ")));
    out.push_str(&format!("Links: {}\n", links.join(" · ")));
    out.push_str(&format!("Buttons: {}\n", buttons.join(" · ")));
    out.push_str(&format!("Images: {} total, {missing_alt} without alt text: {}\n", images.len(), images.join(", ")));
    out.push_str(&format!("Forms: {forms} (fields: {inputs})\n"));
    let remaining = max_chars.saturating_sub(out.len()).max(300);
    out.push_str(&format!("Visible text: {}\n", text.chars().take(remaining).collect::<String>()));
    if text.len() < 40 && lower.contains("id=\"root\"") {
        out.push_str("(The page body is nearly empty: it may need JavaScript to render.)\n");
    }
    Outline { text: out, title, ids }
}

/// Same-site links, in-page anchors and images that don't work on the student's site.
async fn broken_links(client: &reqwest::Client, page: &str, html: &str, ids: &HashSet<String>) -> Vec<String> {
    let Ok(base) = reqwest::Url::parse(page) else { return Vec::new() };
    let lower = html.to_ascii_lowercase();
    let mut broken = Vec::new();
    let mut to_check: Vec<(String, reqwest::Url)> = Vec::new();
    let mut seen = HashSet::new();
    for (tag_name, attr_name) in [("<a ", "href"), ("<img ", "src"), ("<link ", "href"), ("<script ", "src")] {
        let mut i = 0;
        while let Some(p) = lower[i..].find(tag_name) {
            let at = i + p;
            let end = lower[at..].find('>').map(|e| at + e + 1).unwrap_or(lower.len());
            i = end;
            let Some(v) = attr(&html[at..end], attr_name) else { continue };
            let v = v.trim().to_string();
            if v.is_empty() || v.starts_with("mailto:") || v.starts_with("tel:") || v.starts_with("javascript:") || v.starts_with("data:") {
                continue;
            }
            if let Some(anchor) = v.strip_prefix('#') {
                if !anchor.is_empty() && !ids.contains(anchor) && seen.insert(v.clone()) {
                    broken.push(format!("Link to \"#{anchor}\" goes nowhere (no element with that id)."));
                }
                continue;
            }
            if let Ok(url) = base.join(&v) {
                if url.host_str() == base.host_str() && seen.insert(url.to_string()) && to_check.len() < MAX_LINK_CHECKS {
                    let what = if tag_name == "<a " { "Link" } else { "File" };
                    to_check.push((format!("{what} \"{v}\""), url));
                }
            }
        }
    }
    let results = futures_util::future::join_all(to_check.iter().map(|(_, url)| async move {
        client.get(url.clone()).timeout(Duration::from_secs(6)).send().await.map(|r| r.status().as_u16()).unwrap_or(0)
    }))
    .await;
    for ((label, _), status) in to_check.into_iter().zip(results) {
        if status >= 400 {
            broken.push(format!("{label} returns {status}."));
        }
    }
    broken.truncate(12);
    broken
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlines_a_page() {
        let html = r##"<html><head><title>Shop</title><style>.x{}</style></head><body>
            <header class="site-header"><nav id="menu"><a href="#hero">Home</a><a href="#products">Products</a></nav></header>
            <section id="hero"><h1>Big Sale</h1><button>Shop now</button><img src="/img/hero.png" alt=""></section>
            <section id="products" class="grid four"><h2>Products</h2></section>
            <footer><p>© 2026</p></footer><script>var hidden = "do not include";</script></body></html>"##;
        let o = outline(html, 5000);
        assert_eq!(o.title, "Shop");
        assert!(o.text.contains("section#hero \"Big Sale\""));
        assert!(o.text.contains("section#products.grid.four \"Products\""));
        assert!(o.text.contains("Home(#hero)"));
        assert!(o.text.contains("\"Shop now\""));
        assert!(o.text.contains("1 without alt text: hero.png"));
        assert!(!o.text.contains("do not include"));
        assert!(o.ids.contains("products"));
    }

    #[test]
    fn parses_console_errors() {
        let log = "[1:2:INFO:CONSOLE:1] \"Uncaught ReferenceError: foo is not defined\", source: http://x/app.js (1)\n[1:2:INFO:CONSOLE:5] \"hello\", source: http://x (5)";
        assert_eq!(console_errors(log), vec!["Uncaught ReferenceError: foo is not defined"]);
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(normalize_url("jane.github.io/shop").unwrap(), "https://jane.github.io/shop");
        assert_eq!(normalize_url("https://shop.netlify.app").unwrap(), "https://shop.netlify.app/");
        assert!(normalize_url("file:///etc/passwd").is_err());
        assert!(normalize_url("javascript:alert(1)").is_err());
    }

    /// Live: renders a real site with the installed Chrome. cargo test live_browser -- --ignored
    #[test]
    #[ignore]
    fn live_browser_inspects_a_site() {
        tauri::async_runtime::block_on(async {
            let client = reqwest::Client::builder().user_agent("PastePilot").build().unwrap();
            let r = inspect(&client, "https://octocat.github.io/", &[VIEWPORTS[0], VIEWPORTS[2]], true).await;
            println!("ok={} rendered={} title={:?} shots={:?} console={:?} broken={:?}", r.ok, r.rendered, r.title,
                r.shots.iter().map(|s| (s.viewport.clone(), s.data_url.len())).collect::<Vec<_>>(), r.console_errors, r.broken_links);
            println!("{}", r.outline);
            assert!(r.ok && r.rendered);
            assert_eq!(r.shots.len(), 2);
            assert!(r.broken_links.is_empty(), "octocat.github.io's anchors use name=, which is valid");
        });
    }
}
