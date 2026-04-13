use std::path::{Path, PathBuf};

/// Discovers and loads project instruction files (like CLAUDE.md).
///
/// Searches from the given directory upward to the filesystem root,
/// collecting instruction files at each level. Files closer to the
/// project root are loaded first; subdirectory overrides appear later.
pub struct InstructionLoader {
    /// File names to search for, in priority order.
    file_names: Vec<String>,
}

impl InstructionLoader {
    /// Create a loader that looks for the given file names.
    ///
    /// ```
    /// use mini_claw_code::InstructionLoader;
    /// let loader = InstructionLoader::new(&["CLAUDE.md", ".mini-claw/instructions.md"]);
    /// ```
    pub fn new(file_names: &[&str]) -> Self {
        Self {
            file_names: file_names.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Create a loader with the default file names: `CLAUDE.md` and
    /// `.mini-claw/instructions.md`.
    pub fn default_files() -> Self {
        Self::new(&["CLAUDE.md", ".mini-claw/instructions.md"])
    }

    /// Discover instruction files starting from `start_dir` and walking
    /// upward. Returns paths in root-first order.
    pub fn discover(&self, start_dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut dir = Some(start_dir.to_path_buf());

        while let Some(current) = dir {
            for name in &self.file_names {
                let candidate = current.join(name);
                if candidate.is_file() {
                    found.push(candidate);
                }
            }
            dir = current.parent().map(|p| p.to_path_buf());
        }

        // Reverse so root-level files come first
        found.reverse();
        found
    }

    /// Load and concatenate all discovered instruction files.
    ///
    /// Each file's content is separated by a header showing the source path.
    pub fn load(&self, start_dir: &Path) -> Option<String> {
        let paths = self.discover(start_dir);
        if paths.is_empty() {
            return None;
        }

        let mut sections = Vec::new();
        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                let content = content.trim().to_string();
                if !content.is_empty() {
                    sections.push(format!(
                        "# Instructions from {}\n\n{}",
                        path.display(),
                        content
                    ));
                }
            }
        }

        if sections.is_empty() {
            None
        } else {
            Some(sections.join("\n\n---\n\n"))
        }
    }

    /// Build a system prompt section from discovered instructions.
    ///
    /// Returns `None` if no instruction files were found.
    pub fn system_prompt_section(&self, start_dir: &Path) -> Option<String> {
        self.load(start_dir).map(|content| {
            format!(
                "The following project instructions were loaded automatically. \
                 Follow them carefully:\n\n{content}"
            )
        })
    }
}
