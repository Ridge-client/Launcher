//! OSINT tool catalog: loading, validation and filtering.

pub mod health;

use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const EMBEDDED_CATALOG: &str = include_str!("../../data/osint_tools.json");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Catalog {
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tool {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub description: String,
}

/// Every value `Tool::kind` can return, in display order.
pub const KINDS: [&str; 5] = [
    "Website",
    "GitHub tool",
    "GitLab tool",
    "Python package",
    "Browser extension",
];

impl Tool {
    /// What the link points to, from its address: a website, or a tool to download and run.
    pub fn kind(&self) -> &'static str {
        let url = self.url.to_lowercase();
        let host = url
            .split("://")
            .nth(1)
            .unwrap_or(&url)
            .split('/')
            .next()
            .unwrap_or("")
            .trim_start_matches("www.");
        match host {
            "github.com" => "GitHub tool",
            "gitlab.com" => "GitLab tool",
            "pypi.org" => "Python package",
            "chromewebstore.google.com" | "addons.mozilla.org" => "Browser extension",
            "chrome.google.com" if url.contains("/webstore") => "Browser extension",
            _ => "Website",
        }
    }
}

/// A tool together with the category it belongs to.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Entry<'a> {
    pub category: &'a str,
    #[serde(flatten)]
    pub tool: &'a Tool,
}

impl Catalog {
    /// Loads the catalog from `path`, or the embedded catalog when `None`.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        match path {
            Some(path) => {
                let raw = std::fs::read_to_string(path)
                    .with_context(|| format!("reading catalog {}", path.display()))?;
                Self::from_json(&raw).with_context(|| format!("parsing catalog {}", path.display()))
            }
            None => Self::from_json(EMBEDDED_CATALOG).context("embedded catalog is invalid"),
        }
    }

    pub fn from_json(raw: &str) -> Result<Self> {
        let catalog: Catalog = serde_json::from_str(raw)?;
        catalog.validate()?;
        Ok(catalog)
    }

    fn validate(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for entry in self.entries() {
            let url = reqwest::Url::parse(&entry.tool.url)
                .with_context(|| format!("tool '{}' has an invalid URL", entry.tool.name))?;
            if !matches!(url.scheme(), "http" | "https") {
                bail!("tool '{}' must use an http(s) URL", entry.tool.name);
            }
            if !seen.insert(entry.tool.name.to_lowercase()) {
                bail!("duplicate tool name '{}'", entry.tool.name);
            }
        }
        Ok(())
    }

    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        self.categories.iter().flat_map(|c| {
            c.tools.iter().map(move |tool| Entry {
                category: &c.name,
                tool,
            })
        })
    }

    /// Entries matching an optional category substring and free-text query (both case-insensitive).
    pub fn filter(&self, category: Option<&str>, query: Option<&str>) -> Vec<Entry<'_>> {
        let category = category.map(str::to_lowercase);
        let query = query.map(str::to_lowercase);
        self.entries()
            .filter(|e| {
                category
                    .as_deref()
                    .is_none_or(|c| e.category.to_lowercase().contains(c))
            })
            .filter(|e| {
                query.as_deref().is_none_or(|q| {
                    e.tool.name.to_lowercase().contains(q)
                        || e.tool.description.to_lowercase().contains(q)
                })
            })
            .collect()
    }

    pub fn find(&self, name: &str) -> Option<Entry<'_>> {
        self.entries()
            .find(|e| e.tool.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_is_valid_and_non_empty() {
        let catalog = Catalog::load(None).unwrap();
        assert!(catalog.categories.len() >= 10);
        assert!(catalog.entries().count() >= 50);
    }

    #[test]
    fn filter_by_category_and_query() {
        let catalog = Catalog::load(None).unwrap();
        let usernames = catalog.filter(Some("username"), None);
        assert!(!usernames.is_empty());
        assert!(usernames
            .iter()
            .all(|e| e.category.to_lowercase().contains("username")));

        let shodan = catalog.filter(None, Some("SHODAN"));
        assert!(shodan.iter().any(|e| e.tool.name == "Shodan"));
        assert!(shodan.len() < catalog.entries().count());
    }

    #[test]
    fn find_is_case_insensitive() {
        let catalog = Catalog::load(None).unwrap();
        assert_eq!(catalog.find("crt").unwrap().tool.url, "https://crt.sh/");
        assert!(catalog.find("does-not-exist").is_none());
    }

    #[test]
    fn kind_from_the_address() {
        let tool = |url: &str| Tool {
            name: "t".into(),
            url: url.into(),
            description: String::new(),
        };
        assert_eq!(
            tool("https://github.com/sherlock-project/sherlock").kind(),
            "GitHub tool"
        );
        assert_eq!(tool("https://www.GitHub.com/x/y").kind(), "GitHub tool");
        assert_eq!(
            tool("https://pypi.org/project/holehe/").kind(),
            "Python package"
        );
        assert_eq!(
            tool("https://chrome.google.com/webstore/detail/shodan/abc").kind(),
            "Browser extension"
        );
        assert_eq!(tool("https://shodan.io/").kind(), "Website");
        assert_eq!(tool("https://pages.github.io/x").kind(), "Website");
    }

    #[test]
    fn every_catalog_tool_has_a_listed_kind() {
        let catalog = Catalog::load(None).unwrap();
        assert!(catalog.entries().all(|e| KINDS.contains(&e.tool.kind())));
    }

    #[test]
    fn rejects_bad_urls_and_duplicates() {
        let bad_scheme = r#"{"categories":[{"name":"X","tools":[{"name":"a","url":"ftp://x"}]}]}"#;
        assert!(Catalog::from_json(bad_scheme).is_err());

        let dup = r#"{"categories":[{"name":"X","tools":[
            {"name":"a","url":"https://a.example"},{"name":"A","url":"https://b.example"}]}]}"#;
        assert!(Catalog::from_json(dup).is_err());
    }
}
