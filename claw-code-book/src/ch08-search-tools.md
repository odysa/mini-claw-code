# Chapter 8: Search Tools

A coding agent that can only read files it already knows about is like a developer who never uses `find` or `grep`. You can hand it a specific file path and it will read it faithfully, but drop it into an unfamiliar codebase and it is blind. It cannot discover which files exist, cannot search for where a function is defined, cannot find all the places a type is used. Without search, the LLM has to guess file paths -- and it will guess wrong.

Search tools fix this. In this chapter you build two: **GlobTool** finds files by name pattern, and **GrepTool** searches file contents by regex. Together they give the LLM the ability to navigate any codebase, no matter how large or unfamiliar. These are the eyes of the agent.

By the end, `cargo test -p claw-code test_ch8` should pass.

## Two tools, two questions

The split between glob and grep maps to two distinct questions the LLM asks when exploring code:

1. **"What files exist?"** -- GlobTool. The LLM knows it wants Rust files, or test files, or config files. It does not know their exact paths. A glob pattern like `**/*.rs` or `tests/*.toml` answers this.

2. **"Where is this thing defined?"** -- GrepTool. The LLM knows a function name, a type, an error message. It needs to find which file and which line contain it. A regex pattern like `fn parse_sse_line` or `struct QueryConfig` answers this.

Claude Code has both as separate tools for exactly this reason. They serve different purposes, take different inputs, and the LLM chooses between them based on what it knows. Merging them into one tool would muddy the interface -- the LLM would have to figure out whether it is doing a name search or a content search, and the parameter schema would be awkward.

---

## GlobTool

GlobTool is the simpler of the two. It takes a glob pattern, optionally scoped to a base directory, and returns all matching file paths.

### File layout

The implementation lives at `src/tools/glob.rs`. Here is the complete code:

```rust
use async_trait::async_trait;
use serde_json::Value;

use crate::types::*;

pub struct GlobTool {
    def: ToolDefinition,
}

impl GlobTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition::new("glob", "Find files matching a glob pattern")
                .param("pattern", "string", "Glob pattern (e.g. \"**/*.rs\")", true)
                .param(
                    "path",
                    "string",
                    "Base directory to search in (default: current directory)",
                    false,
                ),
        }
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolResult> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' argument"))?;

        let base = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let full_pattern = if pattern.starts_with('/') || pattern.starts_with('.') {
            pattern.to_string()
        } else {
            format!("{base}/{pattern}")
        };

        let entries: Vec<String> = glob::glob(&full_pattern)
            .map_err(|e| anyhow::anyhow!("invalid glob pattern: {e}"))?
            .filter_map(|entry| entry.ok())
            .map(|p| p.display().to_string())
            .collect();

        if entries.is_empty() {
            Ok(ToolResult::text("no files matched"))
        } else {
            Ok(ToolResult::text(entries.join("\n")))
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn activity_description(&self, _args: &Value) -> Option<String> {
        Some("Searching files...".into())
    }
}
```

### Walking through the implementation

**The definition.** Two parameters: `pattern` (required) and `path` (optional). The pattern is a standard glob -- `*.rs` for Rust files in the current directory, `**/*.rs` for Rust files recursively, `src/**/*.toml` for TOML files under `src/`. The path sets the base directory; it defaults to `"."` (the current working directory) when omitted.

**Pattern construction.** The `call` method builds the full glob pattern from the base directory and the user-supplied pattern. If the pattern already starts with `/` or `.`, it is treated as an absolute or relative path and used directly. Otherwise, the base directory is prepended: `format!("{base}/{pattern}")`. This means calling with `{"pattern": "*.rs", "path": "/home/user/project"}` produces the glob `/home/user/project/*.rs`.

**The `glob` crate.** We use the `glob` crate (already in `Cargo.toml`) to do the actual matching. `glob::glob()` returns an iterator of `Result<PathBuf>` entries. We `filter_map` with `entry.ok()` to silently skip any paths that fail (permission errors, broken symlinks). The remaining paths are converted to display strings and collected.

**Output format.** Matching paths are joined with newlines -- one path per line. If nothing matches, we return `"no files matched"` rather than an empty string. This matters for the LLM: an explicit "no files matched" message tells it the pattern was valid but found nothing, prompting it to try a different pattern. An empty string would be ambiguous.

**Safety flags.** `is_read_only` returns `true` -- glob only reads the filesystem directory structure, never modifies anything. `is_concurrent_safe` returns `true` -- multiple glob operations can run in parallel without interfering with each other.

---

## GrepTool

GrepTool is more complex. It searches file contents using regex, optionally scoped to a directory and filtered by file type. The output follows the classic grep format: `path:line_no: content`.

### The complete implementation

Here is `src/tools/grep.rs`:

```rust
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

        let search_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let include_pattern = args.get("include").and_then(|v| v.as_str());
        let include_glob = include_pattern
            .map(|p| glob::Pattern::new(p))
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
fn collect_files(
    dir: &Path,
    include: &Option<glob::Pattern>,
    out: &mut Vec<std::path::PathBuf>,
) {
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
```

### Walking through the implementation

There is more going on here, so let's take it piece by piece.

**The definition.** Three parameters: `pattern` (required regex), `path` (optional file or directory), and `include` (optional glob filter for file names). The LLM might call it as `{"pattern": "fn main"}` to search the current directory, or `{"pattern": "TODO", "path": "src/", "include": "*.rs"}` to search only Rust files under `src/`.

**Regex compilation.** The pattern is compiled into a `regex::Regex` upfront. If the LLM provides an invalid regex (missing closing bracket, bad escape), we return an error immediately rather than crashing partway through the search. The `regex` crate handles the full Rust regex syntax -- character classes, quantifiers, alternation, captures.

**The include filter.** The `include` parameter is a glob pattern, not a regex. We compile it into a `glob::Pattern` using the same `glob` crate that powers GlobTool. The `.transpose()` call is an idiomatic Rust pattern for converting `Option<Result<T>>` into `Result<Option<T>>` -- if there is no include pattern, we get `Ok(None)`; if there is one but it is invalid, we get `Err(...)`.

**Three-way path dispatch.** The search path can be a file, a directory, or nonexistent:

- **File**: Search just that one file. The LLM does this when it already knows which file to look in.
- **Directory**: Recursively collect all files (filtered by `include` if provided), sort them for deterministic output, then search each one.
- **Nonexistent**: Return `ToolResult::error(...)`. Note the use of `ToolResult::error` rather than `Err(...)` -- this is the "errors are values" pattern from Chapter 3. The LLM sees `"error: path does not exist: /nonexistent/path"` and can recover, perhaps by trying a different path.

**Output format.** Each match is formatted as `path:line_no: content`, following the classic grep convention. Line numbers are 1-based (humans and LLMs both expect line 1 to be the first line, not line 0). When no matches are found, the tool returns `"no matches found"` -- again, explicit is better than empty.

---

## Helper function design

The two helper functions -- `search_file` and `collect_files` -- are deliberately designed with different signatures. Understanding why reveals practical Rust async patterns.

### `search_file` is async

```rust
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
```

This function reads a file from disk, which is I/O. Using `tokio::fs::read_to_string` instead of `std::fs::read_to_string` keeps the async runtime free to do other work while waiting on the filesystem. In a real agent with concurrent tool execution, this matters -- a slow NFS mount or large file should not block the entire runtime.

The `let Ok(content) = ... else { return; }` pattern is a quiet bailout. If the file cannot be read -- it is binary, it is a symlink to a deleted file, the user lacks permissions -- we silently skip it. This is the right behavior for a search tool. The LLM asked "where does this pattern appear?" and the answer should only include files where we could actually check. Reporting an error for every unreadable file in a directory tree would drown the useful results in noise.

### `collect_files` is sync

```rust
fn collect_files(
    dir: &Path,
    include: &Option<glob::Pattern>,
    out: &mut Vec<std::path::PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
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
```

Directory walking is fast -- it reads metadata, not file contents. Making it async would add complexity (recursive async functions require boxing) without meaningful performance benefit. The sync `std::fs::read_dir` is fine here.

Three details worth noting:

**Hidden directory skipping.** Directories whose names start with `.` are skipped entirely. This excludes `.git`, `.cargo`, `.vscode`, `node_modules` hidden behind a dot-prefix, and similar directories that are almost never what the LLM wants to search. Without this filter, a grep through a project directory would spend most of its time scanning `.git/objects` -- thousands of binary blob files that produce no useful matches.

**The `include` filter.** When present, the glob pattern is matched against the file *name* only (not the full path). This means `"*.rs"` matches `src/main.rs` by checking just `main.rs` against the pattern. This is intuitive -- when the LLM says "search only Rust files," it means files ending in `.rs`, regardless of where they live in the tree.

**The sort.** After collecting all files, the caller sorts them before searching. This ensures deterministic output order. Without sorting, `read_dir` returns entries in filesystem order, which varies across operating systems and even across runs on the same system. Deterministic output makes tests reliable and makes the LLM's experience consistent.

---

## Why two separate tools

You might wonder: why not one `SearchTool` with a mode parameter? The answer comes down to how LLMs make decisions.

When the LLM sees two separate tools in its schema -- one called `glob` described as "find files matching a pattern" and one called `grep` described as "search file contents using regex" -- it can instantly match its intent to the right tool. "I need to find all test files" maps to glob. "I need to find where `parse_sse_line` is defined" maps to grep.

A combined tool with a `mode: "files" | "content"` parameter adds a decision layer. The LLM has to read the schema more carefully, understand the mode field, and get it right. With smaller models, this extra indirection leads to mistakes -- calling the tool in the wrong mode, or omitting the mode parameter entirely.

Claude Code keeps them separate. So do we.

There is also a practical reason: the parameter sets are different. Glob takes a glob pattern and a base path. Grep takes a regex pattern, a path, and an include filter. Merging them would mean the LLM always sees parameters that are irrelevant to what it is doing, which wastes context tokens and increases the chance of confusion.

---

## How Claude Code does it

Our implementations are the essential protocol -- they capture the core behavior in under 200 lines. Claude Code's production versions are considerably more sophisticated.

**Claude Code's Glob** uses ripgrep internally for speed. On large codebases with hundreds of thousands of files, the `glob` crate's pure-Rust implementation can be slow. Ripgrep's directory walker is optimized for this use case, respecting `.gitignore` rules and parallelizing the walk. Claude Code's Glob also supports sorting results by modification time (most recently changed files first, which is often what the LLM wants) and limits the number of results to avoid flooding the context window.

**Claude Code's Grep** is equally enhanced. It supports context lines (`-A`, `-B`, `-C` flags) to show surrounding code, which helps the LLM understand matches without making a separate `read` call. It offers multiple output modes: show matching lines (default), show only file paths (for counting), or show match counts per file. File type filtering uses ripgrep's built-in type system rather than a glob pattern, so `--type rust` knows about `.rs` files, `Cargo.toml`, and `build.rs` without the user spelling out the glob.

Our versions skip all of this. We use the `glob` crate instead of ripgrep, we have no context lines, no output modes, no result limits. What we do have is the correct protocol: the LLM sends a pattern and gets back matching results in a format it can parse. Everything else is optimization. If you want to upgrade later, the `Tool` trait interface stays the same -- only the internals of `call()` change.

---

## Tests

Run the chapter 8 tests:

```bash
cargo test -p claw-code test_ch8
```

Here is what each test verifies:

### GlobTool tests

**`test_ch8_glob_find_files`** -- Creates a temp directory with `a.rs`, `b.rs`, and `c.txt`. Globs for `*.rs`. Verifies that both `.rs` files appear in the result and the `.txt` file does not.

**`test_ch8_glob_recursive`** -- Creates a temp directory with `top.rs` at the root and `sub/deep.rs` in a subdirectory. Globs for `**/*.rs`. Verifies that both files are found, confirming recursive descent works.

**`test_ch8_glob_no_matches`** -- Creates a temp directory with `file.txt` and globs for `*.xyz`. Verifies the result contains `"no files matched"`.

**`test_ch8_glob_is_read_only`** -- Checks that `is_read_only()` and `is_concurrent_safe()` both return `true`.

**`test_ch8_glob_definition`** -- Verifies the tool definition has the name `"glob"`.

### GrepTool tests

**`test_ch8_grep_single_file`** -- Creates a file containing `fn main()` and `println!("hello")`. Greps for `"println"`. Verifies the match includes the content and the correct line number (`:2:`).

**`test_ch8_grep_directory`** -- Creates two files, both containing `fn foo()`. Greps the directory for `"fn foo"`. Verifies both files appear in the results.

**`test_ch8_grep_with_include`** -- Creates `code.rs` and `data.txt`, both containing `"hello world"`. Greps with `include: "*.rs"`. Verifies only the `.rs` file appears in results.

**`test_ch8_grep_no_matches`** -- Creates a file and greps for a pattern that does not appear. Verifies the result contains `"no matches found"`.

**`test_ch8_grep_regex`** -- Creates a file with `foo123`, `bar456`, `baz789`. Greps with the regex `\d{3}` (three digits). Verifies all three lines match, confirming real regex support rather than plain string matching.

**`test_ch8_grep_nonexistent_path`** -- Greps a path that does not exist. Verifies the result starts with `"error:"`, confirming the tool returns an error value rather than crashing.

**`test_ch8_grep_is_read_only`** -- Checks that `is_read_only()` and `is_concurrent_safe()` both return `true`.

**`test_ch8_grep_definition`** -- Verifies the tool definition has the name `"grep"`.

---

## Recap

This chapter added two search tools that let the agent discover and navigate code:

- **GlobTool** finds files by name pattern. It takes a glob like `**/*.rs` and returns matching paths, one per line. It uses the `glob` crate for pattern matching and defaults to the current directory when no base path is provided.

- **GrepTool** searches file contents by regex. It takes a pattern like `fn main` and returns matches in `path:line_no: content` format. It supports scoping to a file or directory and filtering by file type with the `include` parameter. Two helper functions split the work: `search_file` (async, handles I/O) and `collect_files` (sync, walks the directory tree).

- **Both tools are read-only and concurrent-safe.** They never modify the filesystem, and multiple searches can run in parallel without interfering. The permission system (Chapter 10) will auto-approve them in every mode, including plan mode.

- **The separation is deliberate.** Glob answers "what files exist?" Grep answers "where is this content?" Two tools with clear purposes are easier for the LLM to use correctly than one tool with a mode switch.

With search tools in place, the agent can now explore an unfamiliar codebase on its own. Given a prompt like "find and fix the bug in the parser," it can glob for source files, grep for the parser code, read the relevant files, and then use the write and edit tools from Chapter 6 to make changes. The tool suite is becoming complete.
