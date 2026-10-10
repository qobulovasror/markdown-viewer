//! Documents from http(s) URLs and GitHub shorthands (`gh:owner/repo`).

use std::time::Duration;

use anyhow::{Context, Result, bail};

/// Largest document that is downloaded.
const MAX_BYTES: u64 = 10 << 20;

pub use crate::links::is_remote;

/// Where a GitHub input points.
#[derive(Debug, PartialEq, Eq)]
enum Github {
    /// README of a repo, optionally of a directory at a ref.
    Readme {
        repo: String,
        dir: String,
        git_ref: Option<String>,
    },
    /// A file, as a raw.githubusercontent.com URL.
    Raw(String),
}

/// Parses `gh:o/r[/path]`, `github.com/o/r[/...]` and `https://github.com/o/r/...`.
fn github(input: &str) -> Option<Github> {
    let rest = input
        .strip_prefix("gh:")
        .or_else(|| input.strip_prefix("https://github.com/"))
        .or_else(|| input.strip_prefix("http://github.com/"))
        .or_else(|| input.strip_prefix("https://www.github.com/"))
        .or_else(|| input.strip_prefix("http://www.github.com/"))
        .or_else(|| input.strip_prefix("github.com/"))?;
    let shorthand = input.starts_with("gh:");
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    let (owner, name) = (parts.first()?, parts.get(1)?);
    let repo = format!("{owner}/{}", name.trim_end_matches(".git"));
    let tail = &parts[2..];
    match tail {
        [] => Some(Github::Readme {
            repo,
            dir: String::new(),
            git_ref: None,
        }),
        ["blob" | "raw", git_ref, path @ ..] if !shorthand && !path.is_empty() => {
            Some(Github::Raw(format!(
                "https://raw.githubusercontent.com/{repo}/{git_ref}/{}",
                path.join("/")
            )))
        }
        ["tree", git_ref, dir @ ..] if !shorthand => Some(Github::Readme {
            repo,
            dir: dir.join("/"),
            git_ref: Some(git_ref.to_string()),
        }),
        path if shorthand => Some(Github::Raw(format!(
            "https://raw.githubusercontent.com/{repo}/HEAD/{}",
            path.join("/")
        ))),
        _ => None,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .timeout_connect(Some(Duration::from_secs(5)))
        .user_agent(concat!("mdvw/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str, accept: &str) -> Result<(String, Vec<u8>)> {
    use ureq::ResponseExt;
    let mut resp = agent
        .get(url)
        .header("Accept", accept)
        .call()
        .with_context(|| format!("cannot fetch {url}"))?;
    let final_url = resp.get_uri().to_string();
    let body = resp
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .with_context(|| format!("cannot read {url}"))?;
    Ok((final_url, body))
}

/// Downloads a document; returns its final URL (after redirects) and text.
pub fn fetch(input: &str) -> Result<(String, String)> {
    let agent = agent();
    let url = match github(input) {
        Some(Github::Raw(url)) => url,
        Some(Github::Readme { repo, dir, git_ref }) => {
            let mut api = url::Url::parse("https://api.github.com/repos/").expect("valid URL");
            if let Ok(mut segs) = api.path_segments_mut() {
                segs.pop_if_empty();
                segs.extend(repo.split('/'));
                segs.push("readme");
                segs.extend(dir.split('/').filter(|d| !d.is_empty()));
            }
            if let Some(r) = git_ref {
                api.query_pairs_mut().append_pair("ref", &r);
            }
            let (_, body) = get(&agent, api.as_str(), "application/vnd.github+json")?;
            let json: serde_json::Value =
                serde_json::from_slice(&body).context("unexpected GitHub API response")?;
            json["download_url"]
                .as_str()
                .context("GitHub did not return a README URL")?
                .to_string()
        }
        None if url::Url::parse(input).is_ok_and(|u| matches!(u.scheme(), "http" | "https")) => {
            input.into()
        }
        None => bail!("not a URL or GitHub repository: {input}"),
    };
    let (final_url, body) = get(&agent, &url, "text/markdown, text/plain, */*")?;
    Ok((final_url, String::from_utf8_lossy(&body).into_owned()))
}

/// Resolves a relative link against a document URL (`None` if not http(s)).
pub fn join(base: &str, link: &str) -> Option<String> {
    let url = url::Url::parse(base).ok()?.join(link).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

/// Document URL and anchor to open in-app for a web link to a viewable file,
/// e.g. a GitHub `blob` page or `https://host/notes.md#usage`.
pub fn document_url(link: &str) -> Option<(String, Option<String>)> {
    let url = url::Url::parse(link).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let anchor = url.fragment().filter(|f| !f.is_empty()).map(str::to_string);
    let mut target = match github(link) {
        Some(Github::Raw(raw)) => url::Url::parse(&raw).ok()?,
        _ => url,
    };
    target.set_fragment(None);
    crate::links::is_viewable(std::path::Path::new(target.path()))
        .then(|| (target.to_string(), anchor))
}

/// File name part of a URL path, for the status bar and format detection.
pub fn file_name(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()?
                .rev()
                .find(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_shorthands() {
        let readme = |dir: &str, r: Option<&str>| Github::Readme {
            repo: "o/r".into(),
            dir: dir.into(),
            git_ref: r.map(str::to_string),
        };
        assert_eq!(github("gh:o/r"), Some(readme("", None)));
        assert_eq!(github("github.com/o/r.git/"), Some(readme("", None)));
        assert_eq!(
            github("https://github.com/o/r/tree/dev/docs"),
            Some(readme("docs", Some("dev")))
        );
        assert_eq!(
            github("gh:o/r/docs/a.md"),
            Some(Github::Raw(
                "https://raw.githubusercontent.com/o/r/HEAD/docs/a.md".into()
            ))
        );
        assert_eq!(
            github("https://github.com/o/r/blob/main/docs/a.md#usage"),
            Some(Github::Raw(
                "https://raw.githubusercontent.com/o/r/main/docs/a.md".into()
            ))
        );
        assert_eq!(github("https://github.com/o/r/issues/1"), None);
        assert_eq!(github("gh:o"), None);
        assert_eq!(github("https://example.com/o/r"), None);
    }

    #[test]
    fn joins_and_names() {
        let base = "https://raw.githubusercontent.com/o/r/HEAD/docs/a.md";
        assert_eq!(
            join(base, "../README.md").as_deref(),
            Some("https://raw.githubusercontent.com/o/r/HEAD/README.md")
        );
        assert_eq!(join(base, "javascript:alert(1)"), None);
        assert_eq!(file_name(base), "a.md");
        assert_eq!(
            document_url("https://github.com/o/r/blob/v1/x.md#use"),
            Some((
                "https://raw.githubusercontent.com/o/r/v1/x.md".into(),
                Some("use".into())
            ))
        );
        assert_eq!(document_url("https://example.com/page.html"), None);
        assert_eq!(
            document_url("https://github.com/o/r/blob/v1/logo.png"),
            None
        );
        assert!(document_url("https://example.com/notes.md").is_some());
        assert!(is_remote("gh:o/r") && is_remote("HTTPS://x") && !is_remote("README.md"));
    }
}
