use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct Location {
    pub file: PathBuf,
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub imports_by_package: HashMap<String, Vec<Location>>,
}

const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    "dist",
    "build",
    "Pods",
    ".gradle",
    ".expo",
    ".next",
    ".turbo",
    "coverage",
    "target",
    ".cache",
    ".idea",
    ".vscode",
];

const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.') && s != "." && s != "..")
        .unwrap_or(false)
        && entry.depth() > 0
        && entry.file_type().is_dir()
}

fn is_ignored_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = match entry.file_name().to_str() {
        Some(n) => n,
        None => return false,
    };
    IGNORED_DIRS.contains(&name) || is_hidden(entry)
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SOURCE_EXTENSIONS.contains(&e))
        .unwrap_or(false)
}

fn import_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)(?:from\s+|require\s*\(\s*|import\s+)['"]([^'"./][^'"]*)['"]"#)
            .expect("valid regex")
    })
}

pub fn extract_package_name(import: &str) -> Option<String> {
    if import.is_empty() || import.starts_with('.') || import.starts_with('/') {
        return None;
    }
    if import.starts_with('@') {
        let mut iter = import.splitn(3, '/');
        let scope = iter.next()?;
        let name = iter.next()?;
        if scope.len() < 2 || name.is_empty() {
            return None;
        }
        Some(format!("{scope}/{name}"))
    } else {
        Some(import.split('/').next()?.to_string())
    }
}

fn scan_file(path: &Path, result: &mut ScanResult) -> Result<()> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    result.files_scanned += 1;

    let re = import_regex();
    for (line_idx, line) in content.lines().enumerate() {
        for cap in re.captures_iter(line) {
            if let Some(m) = cap.get(1) {
                if let Some(pkg) = extract_package_name(m.as_str()) {
                    result
                        .imports_by_package
                        .entry(pkg)
                        .or_default()
                        .push(Location {
                            file: path.to_path_buf(),
                            line: line_idx + 1,
                        });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_package() {
        assert_eq!(extract_package_name("lodash"), Some("lodash".into()));
    }

    #[test]
    fn scoped_package() {
        assert_eq!(
            extract_package_name("@react-native-async-storage/async-storage"),
            Some("@react-native-async-storage/async-storage".into())
        );
    }

    #[test]
    fn scoped_with_subpath() {
        assert_eq!(
            extract_package_name("@legendapp/list/reanimated"),
            Some("@legendapp/list".into())
        );
    }

    #[test]
    fn bare_with_subpath() {
        assert_eq!(
            extract_package_name("react-native/Libraries/Foo"),
            Some("react-native".into())
        );
    }

    #[test]
    fn relative_import_skipped() {
        assert_eq!(extract_package_name("./foo"), None);
        assert_eq!(extract_package_name("../../bar"), None);
        assert_eq!(extract_package_name("/abs/path"), None);
    }

    #[test]
    fn empty_string_skipped() {
        assert_eq!(extract_package_name(""), None);
    }

    #[test]
    fn import_regex_captures_various_shapes() {
        let re = import_regex();
        let samples = [
            (r#"import X from "foo""#, Some("foo")),
            (r#"import X from 'foo'"#, Some("foo")),
            (r#"import 'side-effect'"#, Some("side-effect")),
            (r#"const x = require("foo")"#, Some("foo")),
            (r#"const x = require ( 'foo' )"#, Some("foo")),
        ];
        for (src, expected) in samples {
            let cap = re.captures(src).and_then(|c| c.get(1)).map(|m| m.as_str());
            assert_eq!(cap, expected, "input: {src}");
        }
    }
}

pub fn scan_project(root: &Path) -> Result<ScanResult> {
    let mut result = ScanResult::default();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().is_file() && is_source_file(entry.path()) {
            let _ = scan_file(entry.path(), &mut result);
        }
    }

    Ok(result)
}
