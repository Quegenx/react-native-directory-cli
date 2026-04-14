use anyhow::{Context, Result, anyhow};
use owo_colors::OwoColorize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::api::Library;
use crate::cache;
use crate::stopwords::TOPIC_STOPWORDS;
use crate::output;
use crate::scanner::{self, Location, ScanResult};
use crate::ui;

fn load_all(refresh: bool) -> Result<Vec<Library>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(cache::load(refresh))
}

fn emit(libs: &[Library], json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(libs).unwrap_or_default());
    } else {
        for lib in libs {
            println!("{}\n", output::row(lib));
        }
        println!("{} {} result(s)", "→".dimmed(), libs.len());
    }
}

pub fn search(query: &str, limit: usize, refresh: bool, json: bool) -> Result<()> {
    let libs = load_all(refresh)?;
    let mut hits: Vec<&Library> = libs
        .iter()
        .filter(|l| l.matches_query(query))
        .collect();
    hits.sort_by(|a, b| {
        b.stars()
            .cmp(&a.stars())
            .then(b.weekly_downloads().cmp(&a.weekly_downloads()))
    });
    let top: Vec<Library> = hits.into_iter().take(limit).cloned().collect();
    emit(&top, json);
    Ok(())
}

pub fn info(name: &str, refresh: bool, json: bool) -> Result<()> {
    let libs = load_all(refresh)?;
    let lib = libs
        .iter()
        .find(|l| l.name().eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow!("package '{name}' not found in directory"))?;
    if json {
        println!("{}", serde_json::to_string_pretty(lib)?);
    } else {
        output::detailed(lib);
    }
    Ok(())
}

fn normalize_topic(t: &str) -> String {
    let lower = t.to_lowercase();
    let trimmed = lower.trim_end_matches('s');
    if trimmed.len() >= 3 {
        trimmed.to_string()
    } else {
        lower
    }
}

fn meaningful_topics(lib: &Library) -> HashSet<String> {
    let stop: HashSet<&str> = TOPIC_STOPWORDS.iter().copied().collect();
    lib.github
        .as_ref()
        .map(|g| {
            g.topics
                .iter()
                .map(|t| normalize_topic(t))
                .filter(|t| !stop.contains(t.as_str()) && t.len() > 1)
                .collect()
        })
        .unwrap_or_default()
}

fn name_tokens(lib: &Library) -> HashSet<String> {
    let name = lib.name().to_lowercase();
    let stripped = name
        .strip_prefix("@react-native-community/")
        .or_else(|| name.strip_prefix("@react-native-async-storage/"))
        .or_else(|| name.strip_prefix("@react-native/"))
        .or_else(|| name.strip_prefix("react-native-"))
        .or_else(|| name.strip_prefix("expo-"))
        .unwrap_or(&name);
    stripped
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 2)
        .filter(|s| !TOPIC_STOPWORDS.contains(s))
        .map(|s| s.to_string())
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum MatchReason {
    Directory,
    Topic,
    Name,
    Description,
}

fn stem_5(w: &str) -> String {
    // Naive stemmer: strip common suffixes, then truncate to 5 chars
    // so variants like animated/animation/animations collapse to the same stem.
    let trimmed = w
        .trim_end_matches('s')
        .trim_end_matches("tion")
        .trim_end_matches("ing")
        .trim_end_matches("ed")
        .trim_end_matches('e');
    trimmed.chars().take(5).collect()
}

fn description_tokens(lib: &Library) -> HashSet<String> {
    let stop: HashSet<&str> = TOPIC_STOPWORDS.iter().copied().collect();
    const COMMON_WORDS: &[&str] = &[
        "with", "this", "that", "from", "your", "into", "the", "and", "for", "you",
        "are", "can", "use", "has", "have", "more", "most", "some", "over",
        "build", "built", "make", "made", "work", "works", "easy", "simple",
        "using", "providing", "provide", "support", "supports", "package",
        "lightweight", "powerful", "fully", "fast", "small", "big", "new", "old",
    ];
    let common: HashSet<String> = COMMON_WORDS.iter().map(|w| stem_5(w)).collect();
    let desc = lib.description().to_lowercase();
    desc.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 4)
        .filter(|w| !stop.contains(*w))
        .map(stem_5)
        .filter(|s| s.len() >= 4)
        .filter(|s| !common.contains(s))
        .collect()
}

struct Candidate<'a> {
    lib: &'a Library,
    score: f64,
    reason: MatchReason,
}

/// Compute alternative-candidate names for a package, using the same multi-tier
/// algorithm as the `alternatives` command. Returned in rank order.
/// Used by both `alternatives` (for rendering) and `analyze` (for cross-referencing
/// against the project's own deps).
fn compute_candidate_names(target: &Library, libs: &[Library], limit: usize) -> Vec<(String, MatchReason)> {
    let target_topics = meaningful_topics(target);
    let target_tokens = name_tokens(target);
    let target_desc_tokens = description_tokens(target);
    let target_native = target.has_native_code();

    let mut out: Vec<(String, MatchReason)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Tier 1 — directory-curated alternatives.
    if let Some(curated) = &target.alternatives {
        for name in curated {
            if let Some(lib) = libs.iter().find(|l| l.name().eq_ignore_ascii_case(name)) {
                if seen.insert(lib.name().to_string()) {
                    out.push((lib.name().to_string(), MatchReason::Directory));
                }
            }
        }
    }

    let passes_hard_filters = |l: &&Library| -> bool {
        l.name() != target.name()
            && !l.is_archived()
            && !l.unmaintained
            && (!target_native || l.has_native_code())
    };

    // Tier 2 — topic Jaccard.
    let mut topic_hits: Vec<(f64, String)> = libs
        .iter()
        .filter(passes_hard_filters)
        .filter(|l| !seen.contains(l.name()))
        .filter_map(|l| {
            let topics = meaningful_topics(l);
            if topics.is_empty() || target_topics.is_empty() { return None; }
            let overlap = topics.intersection(&target_topics).count();
            if overlap == 0 { return None; }
            let union = topics.union(&target_topics).count().max(1);
            let jaccard = overlap as f64 / union as f64;
            if overlap < 2 && jaccard < 0.2 { return None; }
            let score = jaccard * 1000.0 + l.score.unwrap_or(0.0) + (l.stars().max(1) as f64).ln();
            Some((score, l.name().to_string()))
        })
        .collect();
    topic_hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, name) in topic_hits {
        if out.len() >= limit * 2 { break; }
        if seen.insert(name.clone()) {
            out.push((name, MatchReason::Topic));
        }
    }

    // Tier 3 — name tokens.
    if out.len() < limit && !target_tokens.is_empty() {
        let mut name_hits: Vec<(f64, String)> = libs
            .iter()
            .filter(passes_hard_filters)
            .filter(|l| !seen.contains(l.name()))
            .filter_map(|l| {
                let tokens = name_tokens(l);
                let overlap = tokens.intersection(&target_tokens).count();
                if overlap == 0 { return None; }
                let score = (overlap as f64) * 100.0 + l.score.unwrap_or(0.0) + (l.stars().max(1) as f64).ln();
                Some((score, l.name().to_string()))
            })
            .collect();
        name_hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, name) in name_hits {
            if out.len() >= limit { break; }
            if seen.insert(name.clone()) {
                out.push((name, MatchReason::Name));
            }
        }
    }

    // Tier 4 — description keywords.
    if out.len() < limit && !target_desc_tokens.is_empty() {
        let mut desc_hits: Vec<(f64, String)> = libs
            .iter()
            .filter(passes_hard_filters)
            .filter(|l| !seen.contains(l.name()))
            .filter_map(|l| {
                let tokens = description_tokens(l);
                let overlap = tokens.intersection(&target_desc_tokens).count();
                if overlap == 0 { return None; }
                let union = tokens.union(&target_desc_tokens).count().max(1);
                let jaccard = overlap as f64 / union as f64;
                let min_jaccard = if target_desc_tokens.len() <= 3 { 0.15 } else { 0.2 };
                let min_overlap = if target_desc_tokens.len() <= 3 { 1 } else { 2 };
                if overlap < min_overlap || jaccard < min_jaccard { return None; }
                let score = jaccard * 500.0 + l.score.unwrap_or(0.0) + (l.stars().max(1) as f64).ln();
                Some((score, l.name().to_string()))
            })
            .collect();
        desc_hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, name) in desc_hits {
            if out.len() >= limit { break; }
            if seen.insert(name.clone()) {
                out.push((name, MatchReason::Description));
            }
        }
    }

    out.truncate(limit);
    out
}

pub fn alternatives(name: &str, limit: usize, refresh: bool, json: bool) -> Result<()> {
    let libs = load_all(refresh)?;
    let target = libs
        .iter()
        .find(|l| l.name().eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow!("package '{name}' not found in directory"))?
        .clone();

    let candidate_names = compute_candidate_names(&target, &libs, limit);
    let chosen: Vec<Candidate> = candidate_names
        .iter()
        .filter_map(|(n, reason)| {
            libs.iter().find(|l| l.name() == n).map(|lib| Candidate {
                lib,
                score: 0.0,
                reason: *reason,
            })
        })
        .collect();

    if json {
        let rows: Vec<serde_json::Value> = chosen
            .iter()
            .map(|c| {
                let reason = match c.reason {
                    MatchReason::Directory => "directory",
                    MatchReason::Topic => "topic",
                    MatchReason::Name => "name",
                    MatchReason::Description => "description",
                };
                let mut obj = serde_json::to_value(c.lib).unwrap_or(serde_json::json!({}));
                if let Some(m) = obj.as_object_mut() {
                    m.insert("_match".into(), serde_json::json!(reason));
                }
                obj
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if chosen.is_empty() {
        ui::log::warn(format!("no candidates found for {}", target.name()));
        return Ok(());
    }

    println!("Candidates matching {}:\n", ui::hi::bold(&target.name()));

    for c in &chosen {
        let tag = match c.reason {
            MatchReason::Directory => ui::hi::success("[directory]"),
            MatchReason::Topic => ui::hi::info("[topic]"),
            MatchReason::Name => ui::hi::dim("[name]"),
            MatchReason::Description => ui::hi::dim("[desc]"),
        };
        println!("{} {}\n", tag, output::row(c.lib));
    }
    println!("{} {} result(s)", ui::hi::dim("→"), chosen.len());
    Ok(())
}

pub fn trending(limit: usize, refresh: bool, include_js: bool, json: bool) -> Result<()> {
    let libs = load_all(refresh)?;
    let mut list: Vec<&Library> = libs
        .iter()
        .filter(|l| !l.is_archived() && !l.unmaintained)
        .filter(|l| include_js || l.has_native_code())
        .collect();
    list.sort_by(|a, b| b.weekly_downloads().cmp(&a.weekly_downloads()));
    let top: Vec<Library> = list.into_iter().take(limit).cloned().collect();
    emit(&top, json);
    Ok(())
}

/// Surface actively-maintained, highly-rated native packages — the directory's
/// own curation signals combined with recent activity. Useful for finding
/// "quietly great" libraries that don't dominate raw download charts.
pub fn discover(limit: usize, min_score: f64, days: u32, refresh: bool, json: bool) -> Result<()> {
    let libs = load_all(refresh)?;
    let now = chrono_unix_now();
    let threshold_secs = (days as i64) * 86_400;
    let mut list: Vec<&Library> = libs
        .iter()
        .filter(|l| !l.is_archived() && !l.unmaintained)
        .filter(|l| l.has_native_code())
        .filter(|l| l.score.unwrap_or(0.0) >= min_score)
        .filter(|l| {
            let Some(pushed) = l.pushed_at() else { return false };
            let Some(pushed_secs) = parse_iso8601_secs(pushed) else { return false };
            now - pushed_secs <= threshold_secs
        })
        .collect();
    list.sort_by(|a, b| {
        let sa = a.score.unwrap_or(0.0);
        let sb = b.score.unwrap_or(0.0);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.stars().cmp(&a.stars()))
    });
    let top: Vec<Library> = list.into_iter().take(limit).cloned().collect();
    emit(&top, json);
    Ok(())
}

fn chrono_unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse a subset of ISO 8601: `YYYY-MM-DDTHH:MM:SSZ` → unix seconds.
/// Not full RFC3339 — just enough for the `pushed_at` field shape the API uses.
fn parse_iso8601_secs(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let minute: i64 = s.get(14..16)?.parse().ok()?;
    let second: i64 = s.get(17..19)?.parse().ok()?;
    // Days-from-Epoch algorithm (Howard Hinnant's date.h).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = month as u64;
    let d = day as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

pub struct ListFilters<'a> {
    pub category: Option<&'a str>,
    pub platforms: &'a [String],
    pub works_with: &'a [String],
    pub module_type: Option<&'a str>,
    pub new_arch: bool,
    pub has_types: bool,
    pub native: bool,
    pub config_plugin: bool,
    pub nightly: bool,
    pub include_unmaintained: bool,
    pub no_dev: bool,
    pub limit: usize,
    pub refresh: bool,
    pub json: bool,
}

fn platform_supported(lib: &Library, name: &str) -> bool {
    match name.to_lowercase().as_str() {
        "ios" => lib.ios,
        "android" => lib.android,
        "web" => lib.web,
        "macos" => lib.macos,
        "tvos" => lib.tvos,
        "visionos" => lib.visionos,
        "windows" => lib.windows,
        _ => false,
    }
}

fn works_with_target(lib: &Library, name: &str) -> bool {
    match name.to_lowercase().replace('_', "-").as_str() {
        "expo-go" | "expogo" => lib.expo_go,
        "fireos" => lib.fireos,
        "horizon" => lib.horizon,
        "vegaos" => lib.vegaos.as_ref().map(|v| v.is_truthy()).unwrap_or(false),
        _ => false,
    }
}

pub fn list(f: ListFilters) -> Result<()> {
    let libs = load_all(f.refresh)?;
    let module_type_lc = f.module_type.map(|s| s.to_lowercase());

    let mut filtered: Vec<&Library> = libs
        .iter()
        .filter(|l| {
            // Default: hide archived and unmaintained unless user opts in.
            if !f.include_unmaintained && (l.is_archived() || l.unmaintained) {
                return false;
            }
            if f.no_dev && l.dev {
                return false;
            }
            if let Some(cat) = f.category {
                let cat = cat.to_lowercase();
                let in_topics = l
                    .github
                    .as_ref()
                    .map(|g| g.topics.iter().any(|t| t.to_lowercase().contains(&cat)))
                    .unwrap_or(false);
                if !(in_topics || l.topics_flat.to_lowercase().contains(&cat)) {
                    return false;
                }
            }
            // All requested platforms must be supported (AND).
            for p in f.platforms {
                if !platform_supported(l, p) {
                    return false;
                }
            }
            for w in f.works_with {
                if !works_with_target(l, w) {
                    return false;
                }
            }
            if let Some(mt) = &module_type_lc {
                let lib_mt = l
                    .github
                    .as_ref()
                    .and_then(|g| g.module_type.as_deref())
                    .unwrap_or("");
                if !lib_mt.to_lowercase().contains(mt) {
                    return false;
                }
            }
            if f.new_arch && !matches!(l.supports_new_architecture(), Some(true)) {
                return false;
            }
            if f.has_types && !l.has_types() {
                return false;
            }
            if f.native && !l.has_native_code() {
                return false;
            }
            if f.config_plugin
                && !l.config_plugin.as_ref().map(|v| v.is_truthy()).unwrap_or(false)
            {
                return false;
            }
            if f.nightly && !l.nightly_program {
                return false;
            }
            true
        })
        .collect();

    filtered.sort_by(|a, b| {
        b.score.unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.stars().cmp(&a.stars()))
    });
    let top: Vec<Library> = filtered.into_iter().take(f.limit).cloned().collect();
    emit(&top, f.json);
    Ok(())
}

pub fn compare(a: &str, b: &str, refresh: bool, json: bool) -> Result<()> {
    let libs = load_all(refresh)?;
    let find = |n: &str| {
        libs.iter()
            .find(|l| l.name().eq_ignore_ascii_case(n))
            .cloned()
            .ok_or_else(|| anyhow!("package '{n}' not found"))
    };
    let la = find(a)?;
    let lb = find(b)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&[&la, &lb])?);
        return Ok(());
    }

    let rows: [(&str, String, String); 6] = [
        ("name", la.name().to_string(), lb.name().to_string()),
        ("stars", output::format_num(la.stars()), output::format_num(lb.stars())),
        (
            "downloads/wk",
            output::format_num(la.weekly_downloads()),
            output::format_num(lb.weekly_downloads()),
        ),
        (
            "iOS/Android/Web",
            platforms(&la),
            platforms(&lb),
        ),
        (
            "new arch",
            yes_no(matches!(la.supports_new_architecture(), Some(true))),
            yes_no(matches!(lb.supports_new_architecture(), Some(true))),
        ),
        (
            "archived",
            yes_no(la.is_archived()),
            yes_no(lb.is_archived()),
        ),
    ];

    for (k, va, vb) in rows {
        println!("{:<18} {:<30} {}", k.dimmed(), va, vb);
    }
    Ok(())
}

fn platforms(l: &Library) -> String {
    format!(
        "{}/{}/{}",
        yes_no(l.ios),
        yes_no(l.android),
        yes_no(l.web)
    )
}

fn yes_no(b: bool) -> String {
    if b { "yes".to_string() } else { "no".to_string() }
}

pub struct AnalyzeOutcome {
    pub any_finding: bool,
    pub has_unmaintained: bool,
}

#[derive(serde::Serialize)]
struct LocationJson {
    file: String,
    line: usize,
}

impl From<&Location> for LocationJson {
    fn from(l: &Location) -> Self {
        Self {
            file: l.file.display().to_string(),
            line: l.line,
        }
    }
}

#[derive(serde::Serialize)]
struct PackageFinding {
    package: String,
    flags: Vec<String>,
    in_directory: bool,
    github_url: Option<String>,
    license: Option<String>,
    latest_release: Option<String>,
    latest_release_date: Option<String>,
    last_pushed_at: Option<String>,
    stars: u64,
    weekly_downloads: u64,
    directory_score: Option<f64>,
    topics: Vec<String>,
    has_native_code: bool,
    has_types: bool,
    new_architecture: Option<bool>,
    used_in: Vec<LocationJson>,
    /// Packages already in this project's package.json that the algorithm
    /// identified as alternatives for this flagged dep — so the agent can see
    /// "you already have X that could cover this" without having to guess.
    alternatives_in_project: Vec<String>,
}

#[derive(serde::Serialize)]
struct AnalyzeReport {
    files_scanned: usize,
    deps_total: usize,
    deps_flagged: usize,
    package_findings: Vec<PackageFinding>,
}

pub fn analyze(
    project_path: &PathBuf,
    refresh: bool,
    json: bool,
    quiet: bool,
) -> Result<AnalyzeOutcome> {
    let pkg_path = project_path.join("package.json");
    let raw = fs::read_to_string(&pkg_path)
        .with_context(|| format!("reading {}", pkg_path.display()))?;
    let pkg: serde_json::Value = serde_json::from_str(&raw).context("parsing package.json")?;

    let mut deps: Vec<String> = Vec::new();
    for key in ["dependencies", "devDependencies"] {
        if let Some(map) = pkg.get(key).and_then(|v| v.as_object()) {
            for name in map.keys() {
                deps.push(name.clone());
            }
        }
    }

    let libs = load_all(refresh)?;
    let scan = scanner::scan_project(project_path).unwrap_or_default();

    let mut package_findings: Vec<PackageFinding> = Vec::new();
    let mut has_unmaintained = false;

    for dep in &deps {
        let lib = libs.iter().find(|l| l.name() == dep);
        let mut flags: Vec<String> = Vec::new();

        if let Some(lib) = lib {
            if lib.is_archived() {
                flags.push("archived".into());
                has_unmaintained = true;
            }
            if lib.unmaintained {
                flags.push("unmaintained".into());
                has_unmaintained = true;
            }
            // Only flag new-arch + types for packages that actually ship native code.
            if lib.has_native_code() {
                if matches!(lib.supports_new_architecture(), Some(false)) {
                    flags.push("no new-arch support".into());
                }
                if !lib.has_types() {
                    flags.push("no TypeScript types".into());
                }
            }
        }

        if flags.is_empty() {
            continue;
        }

        let used_in = scan
            .imports_by_package
            .get(dep)
            .map(|locs| locs.iter().map(LocationJson::from).collect())
            .unwrap_or_default();

        let github_url = lib.and_then(|l| l.github_url.clone());
        let (license, last_pushed_at) = lib
            .and_then(|l| l.github.as_ref())
            .map(|gh| {
                let lic = gh.license.as_ref().and_then(|x| x.spdx_id.clone());
                let pushed = gh.stats.as_ref().and_then(|s| s.pushed_at.clone());
                (lic, pushed)
            })
            .unwrap_or((None, None));
        let (latest_release, latest_release_date) = lib
            .and_then(|l| l.npm.as_ref())
            .map(|n| (n.latest_release.clone(), n.latest_release_date.clone()))
            .unwrap_or((None, None));
        let topics = lib
            .and_then(|l| l.github.as_ref())
            .map(|g| g.topics.clone())
            .unwrap_or_default();

        let alternatives_in_project: Vec<String> = if let Some(lib) = lib {
            let candidates = compute_candidate_names(lib, &libs, 20);
            candidates
                .iter()
                .filter(|(name, _)| deps.iter().any(|d| d == name))
                .map(|(name, _)| name.clone())
                .collect()
        } else {
            Vec::new()
        };

        package_findings.push(PackageFinding {
            package: dep.clone(),
            flags,
            in_directory: lib.is_some(),
            github_url,
            license,
            latest_release,
            latest_release_date,
            last_pushed_at,
            stars: lib.map(|l| l.stars()).unwrap_or(0),
            weekly_downloads: lib.map(|l| l.weekly_downloads()).unwrap_or(0),
            directory_score: lib.and_then(|l| l.score),
            topics,
            has_native_code: lib.map(|l| l.has_native_code()).unwrap_or(false),
            has_types: lib.map(|l| l.has_types()).unwrap_or(false),
            new_architecture: lib.and_then(|l| l.supports_new_architecture()),
            used_in,
            alternatives_in_project,
        });
    }

    let report = AnalyzeReport {
        files_scanned: scan.files_scanned,
        deps_total: deps.len(),
        deps_flagged: package_findings.len(),
        package_findings,
    };

    let outcome = AnalyzeOutcome {
        any_finding: report.deps_flagged > 0,
        has_unmaintained,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(outcome);
    }

    if quiet && !outcome.any_finding {
        return Ok(outcome);
    }

    print_report_pretty(&scan, &report);
    Ok(outcome)
}

fn print_report_pretty(scan: &ScanResult, report: &AnalyzeReport) {
    ui::log::info(format!(
        "Scanned {} source files, {} deps, {} flagged",
        ui::hi::bold(&scan.files_scanned.to_string()),
        ui::hi::bold(&report.deps_total.to_string()),
        ui::hi::warn(&report.deps_flagged.to_string()),
    ));

    if report.package_findings.is_empty() {
        ui::log::r#break();
        ui::log::success("no issues found");
        return;
    }

    ui::log::r#break();

    for f in &report.package_findings {
        let is_dead = f.flags.iter().any(|x| x == "archived" || x == "unmaintained");
        let badge = if is_dead { ui::hi::error("⚠") } else { ui::hi::warn("•") };
        let used_summary = if f.used_in.is_empty() {
            String::new()
        } else {
            format!(
                " — used in {} {}",
                f.used_in.len(),
                if f.used_in.len() == 1 { "file" } else { "files" }
            )
        };
        println!("{} {}{}", badge, ui::hi::bold(&f.package), ui::hi::dim(&used_summary));
        for flag in &f.flags {
            println!("    - {}", ui::hi::dim(flag));
        }
        if !f.alternatives_in_project.is_empty() {
            println!(
                "    {} {}",
                ui::hi::info("already in deps that could cover this:"),
                f.alternatives_in_project.join(", "),
            );
        }
        if let Some(url) = &f.github_url {
            println!("    {} {}", ui::hi::dim("repo:"), ui::hi::dim(url));
        }
        let mut stats = Vec::new();
        if f.stars > 0 {
            stats.push(format!("★{}", output::format_num(f.stars)));
        }
        if f.weekly_downloads > 0 {
            stats.push(format!("{}/wk", output::format_num(f.weekly_downloads)));
        }
        if let Some(score) = f.directory_score {
            stats.push(format!("score {}", score as i64));
        }
        if let Some(pushed) = &f.last_pushed_at {
            stats.push(format!("pushed {}", &pushed[..pushed.len().min(10)]));
        }
        if !stats.is_empty() {
            println!("    {} {}", ui::hi::dim("stats:"), ui::hi::dim(&stats.join("  ")));
        }
        for loc in f.used_in.iter().take(5) {
            println!("    {}:{}", ui::hi::dim(&loc.file), ui::hi::dim(&loc.line.to_string()));
        }
        if f.used_in.len() > 5 {
            println!("    {} (+{} more)", ui::hi::dim("…"), f.used_in.len() - 5);
        }
    }
}

pub fn cache_info() -> Result<()> {
    let path = cache::location()?;
    println!("{}", path.display());
    if path.exists() {
        let meta = fs::metadata(&path)?;
        let size = meta.len();
        println!("size: {} KB", size / 1024);
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
                println!("age: {}", humantime::format_duration(std::time::Duration::from_secs(age.as_secs())));
            }
        }
    } else {
        println!("(empty)");
    }
    Ok(())
}

pub fn cache_clear() -> Result<()> {
    cache::clear()?;
    ui::log::success("cache cleared");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_normalizes_plural_variants() {
        // The whole point of stem_5 is that animated/animation/animations collapse.
        assert_eq!(stem_5("animated"), stem_5("animation"));
        assert_eq!(stem_5("animations"), stem_5("animated"));
    }

    #[test]
    fn stem_handles_short_and_multibyte() {
        // len(w) < 5 should return intact (after suffix strip)
        assert_eq!(stem_5("cat"), "cat");
        // multi-byte must not panic
        let s = stem_5("高德地图导航");
        assert!(s.chars().count() <= 5);
    }

    #[test]
    fn stem_strips_common_suffixes() {
        assert_eq!(stem_5("navigating"), "navig");
        assert_eq!(stem_5("navigation"), "navig");
        assert_eq!(stem_5("navigate"), "navig");
    }

    #[test]
    fn name_tokens_strips_rn_prefixes() {
        use crate::api::Library;
        let mut lib = Library {
            github_url: None,
            npm_pkg: Some("react-native-mmkv".into()),
            ios: false, android: false, web: false, macos: false, tvos: false,
            visionos: false, windows: false, fireos: false, horizon: false,
            vegaos: None, expo_go: false, expo: false, dev: false,
            unmaintained: false, nightly_program: false, config_plugin: None,
            examples: vec![], images: vec![],
            new_architecture: None,
            score: None, popularity: None,
            score_modifiers: vec![], topics_flat: String::new(),
            alternatives: None, github: None, npm: None,
        };
        let tokens = name_tokens(&lib);
        assert!(tokens.contains("mmkv"), "tokens = {tokens:?}");
        assert!(!tokens.contains("react"));
        assert!(!tokens.contains("native"));

        lib.npm_pkg = Some("@legendapp/list".into());
        let tokens = name_tokens(&lib);
        assert!(tokens.contains("legendapp") || tokens.contains("list"));
    }
}
