use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct GrepTool {
    def: ToolDefinition,
}

impl GrepTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("grep", "Search file contents using a regex pattern")
                .param("pattern", "string", "Regex pattern to search for", true)
                .param(
                    "path",
                    "string",
                    "File or directory to search in (default: current directory)",
                    false,
                )
                .param(
                    "include",
                    "string",
                    "Glob pattern to filter files (e.g. \"*.rs\")",
                    false,
                ),
        }
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' argument"))?;

        let re = regex::Regex::new(pattern)
            .map_err(|e| anyhow::anyhow!("invalid regex pattern: {e}"))?;

        let search_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let include_pattern = args.get("include").and_then(|v| v.as_str());
        let include_glob = include_pattern
            .map(glob::Pattern::new)
            .transpose()
            .map_err(|e| anyhow::anyhow!("invalid include pattern: {e}"))?;

        let path = Path::new(search_path);
        let mut matches = Vec::new();

        if path.is_file() {
            search_file(&re, path, &mut matches).await;
        } else if path.is_dir() {
            let mut entries = Vec::new();
            collect_files(path, &include_glob, &mut entries);
            entries.sort();
            for file_path in entries {
                search_file(&re, &file_path, &mut matches).await;
            }
        } else {
            return Ok(ToolResult::error(format!(
                "path does not exist: {search_path}"
            )));
        }

        if matches.is_empty() {
            Ok(ToolResult::text("no matches found"))
        } else {
            Ok(ToolResult::text(matches.join("\n")))
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Searching content...".into())
    }
}

/// Search a single file for regex matches and append formatted results.
async fn search_file(re: &regex::Regex, path: &Path, matches: &mut Vec<String>) {
    let Ok(content) = tokio::fs::read_to_string(path).await else {
        return; // Skip binary/unreadable files
    };
    let display = path.display();
    for (line_no, line) in content.lines().enumerate() {
        if re.is_match(line) {
            matches.push(format!("{display}:{}: {line}", line_no + 1));
        }
    }
}

/// Recursively collect files from a directory, optionally filtering by glob.
fn collect_files(dir: &Path, include: &Option<glob::Pattern>, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            collect_files(&path, include, out);
        } else if path.is_file() {
            if let Some(glob) = include {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !glob.matches(&name) {
                    continue;
                }
            }
            out.push(path);
        }
    }
}
