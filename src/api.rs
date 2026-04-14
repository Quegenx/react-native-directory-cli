use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const API_URL: &str = "https://reactnative.directory/api/libraries?limit=5000";

/// The API returns `newArchitecture` as `bool | "new-arch-only" | null`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NewArchValue {
    Bool(bool),
    Str(String),
}

impl NewArchValue {
    pub fn supports(&self) -> bool {
        match self {
            NewArchValue::Bool(b) => *b,
            NewArchValue::Str(s) => matches!(s.as_str(), "new-arch-only" | "true"),
        }
    }
}

/// Several directory fields (e.g. `vegaos`, `configPlugin`) are `bool | string | null`.
/// A string value usually points at a fork package name or a plugin URL — either way,
/// a non-empty string means the feature IS present. We normalize to bool for filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoolOrStr {
    Bool(bool),
    Str(String),
}

impl BoolOrStr {
    pub fn is_truthy(&self) -> bool {
        match self {
            BoolOrStr::Bool(b) => *b,
            BoolOrStr::Str(s) => !s.is_empty(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    #[serde(rename = "githubUrl")]
    pub github_url: Option<String>,
    #[serde(rename = "npmPkg")]
    pub npm_pkg: Option<String>,
    #[serde(default)]
    pub ios: bool,
    #[serde(default)]
    pub android: bool,
    #[serde(default)]
    pub web: bool,
    #[serde(default)]
    pub macos: bool,
    #[serde(default)]
    pub tvos: bool,
    #[serde(default)]
    pub visionos: bool,
    #[serde(default)]
    pub windows: bool,
    #[serde(default)]
    pub fireos: bool,
    #[serde(default)]
    pub horizon: bool,
    #[serde(default)]
    pub vegaos: Option<BoolOrStr>,
    #[serde(default, rename = "expoGo")]
    pub expo_go: bool,
    #[serde(default)]
    pub expo: bool,
    #[serde(default)]
    pub dev: bool,
    #[serde(default)]
    pub unmaintained: bool,
    #[serde(default, rename = "nightlyProgram")]
    pub nightly_program: bool,
    #[serde(default, rename = "configPlugin")]
    pub config_plugin: Option<BoolOrStr>,
    #[serde(default)]
    pub examples: Vec<serde_json::Value>,
    #[serde(default)]
    pub images: Vec<serde_json::Value>,
    #[serde(rename = "newArchitecture")]
    pub new_architecture: Option<NewArchValue>,
    pub score: Option<f64>,
    pub popularity: Option<f64>,
    #[serde(rename = "matchingScoreModifiers", default)]
    pub score_modifiers: Vec<String>,
    #[serde(rename = "topicSearchString", default)]
    pub topics_flat: String,
    #[serde(default)]
    pub alternatives: Option<Vec<String>>,
    pub github: Option<GithubInfo>,
    pub npm: Option<NpmInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubInfo {
    pub name: Option<String>,
    #[serde(rename = "fullName")]
    pub full_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    pub license: Option<License>,
    pub stats: Option<GithubStats>,
    #[serde(rename = "isArchived", default)]
    pub is_archived: bool,
    #[serde(rename = "hasTypes", default)]
    pub has_types: bool,
    #[serde(rename = "newArchitecture", default)]
    pub new_architecture: bool,
    #[serde(rename = "hasNativeCode", default)]
    pub has_native_code: bool,
    #[serde(rename = "moduleType")]
    pub module_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub name: Option<String>,
    #[serde(rename = "spdxId")]
    pub spdx_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubStats {
    #[serde(default)]
    pub stars: u64,
    #[serde(default)]
    pub forks: u64,
    #[serde(default)]
    pub issues: u64,
    #[serde(rename = "pushedAt")]
    pub pushed_at: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmInfo {
    #[serde(default)]
    pub downloads: u64,
    #[serde(rename = "weekDownloads", default)]
    pub week_downloads: u64,
    #[serde(rename = "latestRelease")]
    pub latest_release: Option<String>,
    #[serde(rename = "latestReleaseDate")]
    pub latest_release_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    libraries: Vec<Library>,
    #[allow(dead_code)]
    total: Option<u64>,
}

pub async fn fetch_all() -> Result<Vec<Library>> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("rnd/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp: ApiResponse = client
        .get(API_URL)
        .send()
        .await
        .context("failed to reach reactnative.directory")?
        .error_for_status()?
        .json()
        .await
        .context("failed to parse API response")?;
    Ok(resp.libraries)
}

impl Library {
    pub fn name(&self) -> &str {
        self.npm_pkg
            .as_deref()
            .or_else(|| self.github.as_ref().and_then(|g| g.full_name.as_deref()))
            .unwrap_or("<unknown>")
    }

    pub fn description(&self) -> &str {
        self.github
            .as_ref()
            .and_then(|g| g.description.as_deref())
            .unwrap_or("")
    }

    pub fn stars(&self) -> u64 {
        self.github
            .as_ref()
            .and_then(|g| g.stats.as_ref())
            .map(|s| s.stars)
            .unwrap_or(0)
    }

    pub fn weekly_downloads(&self) -> u64 {
        self.npm.as_ref().map(|n| n.week_downloads).unwrap_or(0)
    }

    pub fn pushed_at(&self) -> Option<&str> {
        self.github
            .as_ref()
            .and_then(|g| g.stats.as_ref())
            .and_then(|s| s.pushed_at.as_deref())
    }

    pub fn is_archived(&self) -> bool {
        self.github.as_ref().map(|g| g.is_archived).unwrap_or(false)
    }

    /// Authoritative new-arch support: top-level `newArchitecture` wins
    /// over the older `github.newArchitecture` field.
    pub fn supports_new_architecture(&self) -> Option<bool> {
        if let Some(v) = &self.new_architecture {
            return Some(v.supports());
        }
        self.github.as_ref().map(|g| g.new_architecture)
    }

    pub fn has_native_code(&self) -> bool {
        self.github
            .as_ref()
            .map(|g| g.has_native_code)
            .unwrap_or(false)
    }

    pub fn has_types(&self) -> bool {
        self.github.as_ref().map(|g| g.has_types).unwrap_or(false)
    }

    pub fn matches_query(&self, q: &str) -> bool {
        let q = q.to_lowercase();
        self.name().to_lowercase().contains(&q)
            || self.description().to_lowercase().contains(&q)
            || self.topics_flat.to_lowercase().contains(&q)
    }
}
