//! Read-only GitHub access for public repositories.
//!
//! - File tree: one REST API call (`/git/trees/{ref}?recursive=1`). `HEAD`
//!   resolves the default branch, so no separate repo lookup is needed.
//! - File contents: raw.githubusercontent.com, which doesn't count against
//!   the REST API's 60 requests/hour limit for unauthenticated clients.
//! - Cache: trees for a few minutes, file contents by blob SHA (a SHA always
//!   maps to the same content, so cached files can never be stale).
//!
//! Nothing here can write to GitHub. `token` is plumbing for private repos later.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const API: &str = "https://api.github.com";
const RAW: &str = "https://raw.githubusercontent.com";
const TREE_TTL: Duration = Duration::from_secs(180);
const USER_AGENT: &str = "PastePilot (read-only support assistant)";

#[derive(Debug, Clone, PartialEq)]
pub struct RepoRef {
    pub owner: String,
    pub repo: String,
    /// Branch from a /tree/ or /blob/ URL; None = default branch.
    pub branch: Option<String>,
    /// File or folder from a /tree/ or /blob/ URL.
    pub path: Option<String>,
}

impl RepoRef {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: String,
    pub size: u64,
    pub sha: String,
}

#[derive(Debug, Clone)]
pub struct RepoTree {
    /// The ref used for raw file URLs ("HEAD" or a branch name).
    pub git_ref: String,
    pub entries: Vec<TreeEntry>,
    /// GitHub stops listing very large repositories.
    pub truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    #[serde(skip)]
    pub patch: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub sha: String,
    pub short: String,
    pub message: String,
    pub date: String,
    pub url: String,
    pub files: Vec<CommitFile>,
}

#[derive(Debug)]
pub enum GhError {
    InvalidUrl,
    NotFound,
    EmptyRepo,
    RateLimited { minutes: u64 },
    FileNotFound,
    Binary,
    Network,
    Api(u16),
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            GhError::InvalidUrl => "That doesn't look like a GitHub repository URL.".to_string(),
            GhError::NotFound => "Repository not found. It may be private, or the URL may be wrong.".to_string(),
            GhError::EmptyRepo => "This repository is empty.".to_string(),
            GhError::RateLimited { minutes } => format!(
                "GitHub's limit for anonymous requests was reached. Try again in about {minutes} minute{}.",
                if *minutes == 1 { "" } else { "s" }
            ),
            GhError::FileNotFound => "That file could not be found.".to_string(),
            GhError::Binary => "That file isn't a text file.".to_string(),
            GhError::Network => "Couldn't reach GitHub. Check your connection.".to_string(),
            GhError::Api(code) => format!("GitHub returned an error ({code}). Try again."),
        };
        f.write_str(&msg)
    }
}

/// Accepts https://github.com/owner/repo(.git), /tree/branch/path, /blob/branch/path,
/// git@github.com:owner/repo.git, github.com/owner/repo, or plain owner/repo.
pub fn parse_repo_url(input: &str) -> Result<RepoRef, GhError> {
    let mut s = input.trim().trim_end_matches('/');
    if let Some(rest) = s.strip_prefix("git@github.com:") {
        s = rest;
    } else {
        for prefix in ["https://", "http://"] {
            if let Some(rest) = s.strip_prefix(prefix) {
                s = rest;
            }
        }
        s = s.strip_prefix("www.").unwrap_or(s);
        if let Some(rest) = s.strip_prefix("github.com/") {
            s = rest;
        } else if s.contains('.') && s.split('/').next().is_some_and(|h| h.contains('.')) {
            return Err(GhError::InvalidUrl); // some other host
        }
    }
    let s = s.split(['?', '#']).next().unwrap_or("");
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return Err(GhError::InvalidUrl);
    }
    let valid = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if !valid(owner) || !valid(repo) {
        return Err(GhError::InvalidUrl);
    }
    let (branch, path) = match parts.get(2) {
        Some(&"tree") | Some(&"blob") if parts.len() >= 4 => {
            let path = parts[4..].join("/");
            (Some(parts[3].to_string()), (!path.is_empty()).then_some(path))
        }
        _ => (None, None),
    };
    Ok(RepoRef { owner: owner.to_string(), repo: repo.to_string(), branch, path })
}

#[derive(Default)]
struct Cache {
    trees: HashMap<String, (Instant, RepoTree)>,
    files: HashMap<String, Arc<str>>,
    commits: HashMap<String, (Instant, Vec<CommitInfo>)>,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(Cache::default()));

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

static SHARED: LazyLock<GitHub> = LazyLock::new(|| GitHub::new(None));

/// One client (one connection pool, one cache) for the whole app.
pub fn shared() -> &'static GitHub {
    &SHARED
}

pub struct GitHub {
    client: reqwest::Client,
    token: Option<String>,
}

impl GitHub {
    pub fn new(token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(300))
            .build()
            .expect("http client");
        Self { client, token }
    }

    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        let req = self.client.get(url);
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    /// Lists every file in the repository at `git_ref` ("HEAD" = default branch).
    pub async fn tree(&self, repo: &RepoRef, git_ref: &str) -> Result<RepoTree, GhError> {
        let key = format!("{}@{}", repo.full_name(), git_ref).to_lowercase();
        if let Some((at, tree)) = cache().trees.get(&key) {
            if at.elapsed() < TREE_TTL {
                return Ok(tree.clone());
            }
        }
        let url = format!("{API}/repos/{}/{}/git/trees/{}?recursive=1", repo.owner, repo.repo, encode_path(git_ref));
        let resp = self
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|_| GhError::Network)?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(api_error(status, resp.headers()));
        }
        let body = json_value(resp).await?;
        let entries = body["tree"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|i| i["type"] == "blob")
                    .filter_map(|i| {
                        Some(TreeEntry {
                            path: i["path"].as_str()?.to_string(),
                            size: i["size"].as_u64().unwrap_or(0),
                            sha: i["sha"].as_str().unwrap_or_default().to_string(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let tree = RepoTree { git_ref: git_ref.to_string(), entries, truncated: body["truncated"].as_bool().unwrap_or(false) };
        cache().trees.insert(key, (Instant::now(), tree.clone()));
        Ok(tree)
    }

    /// The `n` most recent commits with their changed files and diffs.
    /// Costs n + 1 API requests, so it's only used when relevant. Cached a few minutes.
    pub async fn recent_commits(&self, repo: &RepoRef, git_ref: &str, n: usize) -> Result<Vec<CommitInfo>, GhError> {
        let key = format!("{}@{}#{}", repo.full_name(), git_ref, n).to_lowercase();
        if let Some((at, commits)) = cache().commits.get(&key) {
            if at.elapsed() < TREE_TTL {
                return Ok(commits.clone());
            }
        }
        let mut url = format!("{API}/repos/{}/{}/commits?per_page={n}", repo.owner, repo.repo);
        if git_ref != "HEAD" {
            url.push_str(&format!("&sha={}", encode_path(git_ref)));
        }
        let list = self.api_json(&url).await?;
        let shas: Vec<String> = list
            .as_array()
            .map(|a| a.iter().filter_map(|c| c["sha"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        let details = futures_util::future::join_all(shas.iter().map(|sha| {
            let url = format!("{API}/repos/{}/{}/commits/{sha}", repo.owner, repo.repo);
            async move { self.api_json(&url).await }
        }))
        .await;
        let commits: Vec<CommitInfo> = details
            .into_iter()
            .flatten()
            .map(|c| {
                let sha = c["sha"].as_str().unwrap_or_default().to_string();
                CommitInfo {
                    short: sha.chars().take(7).collect(),
                    message: c["commit"]["message"].as_str().unwrap_or_default().lines().next().unwrap_or_default().to_string(),
                    date: c["commit"]["author"]["date"].as_str().unwrap_or_default().to_string(),
                    url: c["html_url"].as_str().unwrap_or_default().to_string(),
                    files: c["files"]
                        .as_array()
                        .map(|fs| {
                            fs.iter()
                                .take(40)
                                .map(|f| CommitFile {
                                    path: f["filename"].as_str().unwrap_or_default().to_string(),
                                    status: f["status"].as_str().unwrap_or_default().to_string(),
                                    additions: f["additions"].as_u64().unwrap_or(0),
                                    deletions: f["deletions"].as_u64().unwrap_or(0),
                                    patch: f["patch"].as_str().map(String::from),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    sha,
                }
            })
            .collect();
        cache().commits.insert(key, (Instant::now(), commits.clone()));
        Ok(commits)
    }

    async fn api_json(&self, url: &str) -> Result<serde_json::Value, GhError> {
        let resp = self
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|_| GhError::Network)?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(api_error(status, resp.headers()));
        }
        json_value(resp).await
    }

    /// Raw text of one file. Cached by blob SHA.
    pub async fn file(&self, repo: &RepoRef, git_ref: &str, entry: &TreeEntry) -> Result<Arc<str>, GhError> {
        if let Some(text) = cache().files.get(&entry.sha) {
            return Ok(text.clone());
        }
        let url = format!("{RAW}/{}/{}/{}/{}", repo.owner, repo.repo, encode_path(git_ref), encode_path(&entry.path));
        let resp = self.get(&url).send().await.map_err(|_| GhError::Network)?;
        match resp.status().as_u16() {
            200 => {}
            404 => return Err(GhError::FileNotFound),
            code => return Err(api_error(code, resp.headers())),
        }
        let bytes = resp.bytes().await.map_err(|_| GhError::Network)?;
        if bytes.iter().take(8000).any(|b| *b == 0) {
            return Err(GhError::Binary);
        }
        let text: Arc<str> = String::from_utf8_lossy(&bytes).into_owned().into();
        if !entry.sha.is_empty() {
            cache().files.insert(entry.sha.clone(), text.clone());
        }
        Ok(text)
    }
}

async fn json_value(resp: reqwest::Response) -> Result<serde_json::Value, GhError> {
    let text = resp.text().await.map_err(|_| GhError::Network)?;
    serde_json::from_str(&text).map_err(|_| GhError::Api(200))
}

fn api_error(status: u16, headers: &reqwest::header::HeaderMap) -> GhError {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let remaining = header("x-ratelimit-remaining");
    match status {
        403 | 429 if remaining.as_deref() == Some("0") || status == 429 => {
            let reset = header("x-ratelimit-reset").and_then(|r| r.parse::<u64>().ok()).unwrap_or(0);
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            GhError::RateLimited { minutes: (reset.saturating_sub(now) / 60).max(1) }
        }
        404 => GhError::NotFound,
        409 => GhError::EmptyRepo,
        _ => GhError::Api(status),
    }
}

/// Percent-encodes each path segment, keeping the slashes.
fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            seg.bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Browser link to a file (and lines) for the results view.
pub fn blob_url(repo: &RepoRef, git_ref: &str, path: &str, lines: Option<(u32, u32)>) -> String {
    let mut url = format!("https://github.com/{}/{}/blob/{}/{}", repo.owner, repo.repo, encode_path(git_ref), encode_path(path));
    if let Some((a, b)) = lines {
        url.push_str(&if a == b { format!("#L{a}") } else { format!("#L{a}-L{b}") });
    }
    url
}

/// First GitHub repository URL mentioned in some text (students often paste it).
pub fn find_repo_url(text: &str) -> Option<String> {
    let idx = text.find("github.com/")?;
    let start = text[..idx].rfind(|c: char| c.is_whitespace() || "(<[\"'".contains(c)).map(|i| i + 1).unwrap_or(0);
    let end = text[idx..]
        .find(|c: char| c.is_whitespace() || ")>]\"',".contains(c))
        .map(|i| idx + i)
        .unwrap_or(text.len());
    let candidate = text[start..end].trim_end_matches(['.', '!', '?']);
    let parsed = parse_repo_url(candidate).ok()?;
    Some(format!("https://github.com/{}", parsed.full_name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repo_urls() {
        let r = parse_repo_url("https://github.com/jane/portfolio").unwrap();
        assert_eq!((r.owner.as_str(), r.repo.as_str(), r.branch.clone()), ("jane", "portfolio", None));
        let r = parse_repo_url("github.com/jane/portfolio.git/").unwrap();
        assert_eq!(r.repo, "portfolio");
        let r = parse_repo_url("https://github.com/jane/portfolio/blob/main/src/App.jsx").unwrap();
        assert_eq!(r.branch.as_deref(), Some("main"));
        assert_eq!(r.path.as_deref(), Some("src/App.jsx"));
        let r = parse_repo_url("git@github.com:jane/portfolio.git").unwrap();
        assert_eq!(r.repo, "portfolio");
        assert_eq!(parse_repo_url("jane/portfolio").unwrap().owner, "jane");
        assert!(parse_repo_url("https://gitlab.com/jane/portfolio").is_err());
        assert!(parse_repo_url("https://github.com/jane").is_err());
    }

    #[test]
    fn finds_repo_url_in_messages() {
        assert_eq!(
            find_repo_url("here's my repo: https://github.com/jane/portfolio/tree/main. thanks!").as_deref(),
            Some("https://github.com/jane/portfolio")
        );
        assert_eq!(find_repo_url("no link here"), None);
    }

    #[test]
    fn encodes_paths() {
        assert_eq!(encode_path("src/My File.jsx"), "src/My%20File.jsx");
        assert_eq!(blob_url(&parse_repo_url("a/b").unwrap(), "HEAD", "x.js", Some((3, 5))), "https://github.com/a/b/blob/HEAD/x.js#L3-L5");
    }
}

/// Live check against GitHub (network). Run with: cargo test live_ -- --ignored
#[cfg(test)]
mod live {
    use super::*;
    use crate::repo_search as search;

    #[test]
    #[ignore]
    fn live_public_repo_tree_files_and_imports() {
        tauri::async_runtime::block_on(async {
            let gh = GitHub::new(None);
            let repo = parse_repo_url("https://github.com/octocat/Spoon-Knife").unwrap();
            let tree = gh.tree(&repo, "HEAD").await.expect("tree");
            let files = search::candidates(&tree.entries);
            let names: Vec<&str> = files.iter().map(|e| e.path.as_str()).collect();
            println!("files: {names:?}");
            let html = search::resolve_user_file("index.html", &files).expect("index.html");
            let entry = files.iter().find(|e| e.path == html).unwrap();
            let text = gh.file(&repo, &tree.git_ref, entry).await.expect("raw file");
            assert!(text.contains("<html") || text.contains("<!DOCTYPE"));
            let all: std::collections::HashSet<String> = files.iter().map(|e| e.path.clone()).collect();
            let imports: Vec<String> = search::local_imports(&html, &text)
                .iter()
                .filter_map(|s| search::resolve_import(&html, s, &all))
                .collect();
            println!("index.html imports: {imports:?}");
            assert!(imports.iter().any(|p| p.ends_with(".css")));
            // Second call is served from cache (no network).
            let start = std::time::Instant::now();
            gh.file(&repo, &tree.git_ref, entry).await.unwrap();
            assert!(start.elapsed().as_millis() < 5);
            // Project detection + static checks on real files.
            let css_entry = files.iter().find(|e| e.path == "styles.css").unwrap();
            let css = gh.file(&repo, &tree.git_ref, css_entry).await.unwrap();
            let paths: Vec<&str> = files.iter().map(|e| e.path.as_str()).collect();
            let project = crate::project::detect(&paths, None);
            println!("project: {}", project.label);
            assert!(project.has("vanilla"));
            let every: std::collections::HashSet<String> = tree.entries.iter().map(|e| e.path.clone()).collect();
            let checks = crate::checks::run(&[(html.clone(), text.clone()), ("styles.css".into(), css)], &every, None);
            println!("checks: {checks:?}");
            // Spoon-Knife's index.html really does reference forkit.gif, which isn't in the repo.
            assert_eq!(checks.len(), 1);
            assert!(checks[0].message.contains("forkit.gif"));
            let commits = gh.recent_commits(&repo, &tree.git_ref, 2).await.expect("commits");
            println!("commits: {:?}", commits.iter().map(|c| format!("{} {} ({} files)", c.short, c.message, c.files.len())).collect::<Vec<_>>());
            assert!(!commits.is_empty());
            let missing = gh.tree(&parse_repo_url("octocat/this-repo-does-not-exist-pp").unwrap(), "HEAD").await;
            println!("missing repo: {}", missing.unwrap_err());
        });
    }
}
