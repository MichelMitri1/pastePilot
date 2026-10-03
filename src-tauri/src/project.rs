//! Detects what kind of project a repository is (React + Vite, Next.js,
//! vanilla HTML/CSS, Vue, TypeScript, Tailwind…) from its file list and
//! package.json, so the analysis knows the conventions to check against.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    /// Human label, e.g. "Next.js (App Router) · TypeScript · Tailwind".
    pub label: String,
    /// Machine tags: react, next, vue, vite, typescript, tailwind, vanilla…
    pub tags: Vec<String>,
}

impl ProjectInfo {
    pub fn has(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// All declared dependency names (dependencies + devDependencies + peer).
pub fn dependencies(pkg: &Value) -> Vec<String> {
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .filter_map(|k| pkg[*k].as_object())
        .flat_map(|o| o.keys().cloned())
        .collect()
}

pub fn detect(paths: &[&str], package_json: Option<&Value>) -> ProjectInfo {
    let lower: Vec<String> = paths.iter().map(|p| p.to_lowercase()).collect();
    let has_file = |pred: &dyn Fn(&str) -> bool| lower.iter().any(|p| pred(p));
    let base = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let deps = package_json.map(dependencies).unwrap_or_default();
    let dep = |name: &str| deps.iter().any(|d| d == name);

    let mut tags: Vec<&str> = Vec::new();
    let mut framework = String::new();

    if dep("next") || has_file(&|p| base(p).starts_with("next.config")) {
        tags.push("next");
        tags.push("react");
        let app_router = has_file(&|p| (p.starts_with("app/") || p.starts_with("src/app/")) && base(p).starts_with("page."));
        framework = if app_router { "Next.js (App Router)".into() } else { "Next.js (Pages Router)".into() };
    } else if dep("nuxt") || has_file(&|p| base(p).starts_with("nuxt.config")) {
        tags.extend(["nuxt", "vue"]);
        framework = "Nuxt".into();
    } else if dep("@sveltejs/kit") {
        tags.extend(["sveltekit", "svelte"]);
        framework = "SvelteKit".into();
    } else if dep("svelte") || has_file(&|p| p.ends_with(".svelte")) {
        tags.push("svelte");
        framework = "Svelte".into();
    } else if dep("@angular/core") {
        tags.push("angular");
        framework = "Angular".into();
    } else if dep("vue") || has_file(&|p| p.ends_with(".vue")) {
        tags.push("vue");
        framework = "Vue".into();
    } else if dep("react-native") || dep("expo") {
        tags.extend(["react-native", "react"]);
        framework = if dep("expo") { "React Native (Expo)".into() } else { "React Native".into() };
    } else if dep("react") || has_file(&|p| p.ends_with(".jsx") || p.ends_with(".tsx")) {
        tags.push("react");
        framework = "React".into();
    } else if dep("express") || dep("fastify") || dep("koa") {
        tags.push("node");
        framework = if dep("express") { "Node.js + Express".into() } else { "Node.js".into() };
    } else if has_file(&|p| base(p) == "manage.py") || dep("django") {
        tags.extend(["python", "django"]);
        framework = "Django".into();
    } else if has_file(&|p| p.ends_with(".py")) {
        tags.push("python");
        framework = "Python".into();
    } else if package_json.is_none() && has_file(&|p| p.ends_with(".html")) {
        tags.push("vanilla");
        framework = "HTML/CSS/JavaScript (no framework)".into();
    } else if package_json.is_some() {
        tags.push("node");
        framework = "JavaScript (Node.js)".into();
    }

    let mut extras: Vec<&str> = Vec::new();
    if dep("vite") || has_file(&|p| base(p).starts_with("vite.config")) {
        tags.push("vite");
        extras.push("Vite");
    } else if dep("react-scripts") {
        tags.push("cra");
        extras.push("Create React App");
    }
    if dep("typescript") || has_file(&|p| base(p) == "tsconfig.json" || p.ends_with(".ts") || p.ends_with(".tsx")) {
        tags.push("typescript");
        extras.push("TypeScript");
    }
    if dep("tailwindcss") || has_file(&|p| base(p).starts_with("tailwind.config")) {
        tags.push("tailwind");
        extras.push("Tailwind");
    }
    if dep("react-router-dom") || dep("react-router") {
        tags.push("react-router");
        extras.push("React Router");
    }
    if dep("sass") || has_file(&|p| p.ends_with(".scss")) {
        tags.push("sass");
        extras.push("Sass");
    }
    if dep("bootstrap") {
        tags.push("bootstrap");
        extras.push("Bootstrap");
    }
    if dep("styled-components") {
        tags.push("styled-components");
        extras.push("styled-components");
    }

    let mut parts = Vec::new();
    if !framework.is_empty() {
        parts.push(framework);
    }
    parts.extend(extras.iter().map(|s| s.to_string()));
    ProjectInfo {
        label: if parts.is_empty() { "Unknown project type".into() } else { parts.join(" · ") },
        tags: tags.into_iter().map(String::from).collect(),
    }
}

/// Framework-specific files worth reading for routing/blank-page issues.
pub fn entry_files(info: &ProjectInfo, paths: &[&str]) -> Vec<String> {
    let wanted: &[&str] = if info.has("next") {
        &["app/layout", "app/page", "src/app/layout", "src/app/page", "pages/_app", "pages/index", "src/pages/_app", "src/pages/index"]
    } else if info.has("vue") {
        &["src/main", "src/app", "src/router/index"]
    } else if info.has("vanilla") {
        &["index", "script", "main", "app", "style", "styles"]
    } else {
        &[]
    };
    let mut out = Vec::new();
    for w in wanted {
        if let Some(p) = paths.iter().find(|p| {
            let l = p.to_lowercase();
            let stemmed = l.rsplit_once('.').map(|(s, _)| s).unwrap_or(&l);
            stemmed == *w
        }) {
            out.push(p.to_string());
        }
    }
    out.truncate(3);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_common_stacks() {
        let pkg = json!({"dependencies": {"react": "18", "react-dom": "18", "react-router-dom": "6"}, "devDependencies": {"vite": "5", "typescript": "5", "tailwindcss": "3"}});
        let p = detect(&["src/App.tsx", "index.html", "vite.config.ts"], Some(&pkg));
        assert_eq!(p.label, "React · Vite · TypeScript · Tailwind · React Router");
        assert!(p.has("react") && p.has("vite"));

        let next = detect(&["app/page.tsx", "app/layout.tsx", "next.config.js"], Some(&json!({"dependencies": {"next": "14", "react": "18"}})));
        assert!(next.label.starts_with("Next.js (App Router)"));

        let vanilla = detect(&["index.html", "style.css", "script.js"], None);
        assert_eq!(vanilla.label, "HTML/CSS/JavaScript (no framework)");
        assert_eq!(entry_files(&vanilla, &["index.html", "style.css", "script.js"]), vec!["index.html", "script.js", "style.css"]);
    }
}
