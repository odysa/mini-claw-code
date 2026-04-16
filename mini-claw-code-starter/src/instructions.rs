use std::path::{Path, PathBuf};

/// Discovers and loads project instruction files (like CLAUDE.md).
pub struct InstructionLoader {
    file_names: Vec<String>,
}

impl InstructionLoader {
    pub fn new(file_names: &[&str]) -> Self {
        Self {
            file_names: file_names.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Create a loader with defaults: CLAUDE.md, .mini-claw/instructions.md
    pub fn default_files() -> Self {
        Self::new(&["CLAUDE.md", ".mini-claw/instructions.md"])
    }

    /// Discover instruction files by walking up from start_dir.
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

        found.reverse();
        found
    }

    /// Load and concatenate all discovered files, separated by headers.
    pub fn load(&self, start_dir: &Path) -> Option<String> {
        let files = self.discover(start_dir);
        let mut sections = Vec::new();

        for path in &files {
            if let Ok(content) = std::fs::read_to_string(path)
                && !content.trim().is_empty()
            {
                sections.push(format!(
                    "# Instructions from {}\n\n{}",
                    path.display(),
                    content.trim()
                ));
            }
        }

        if sections.is_empty() {
            None
        } else {
            Some(sections.join("\n\n---\n\n"))
        }
    }

    /// Build a system prompt section from discovered instructions.
    pub fn system_prompt_section(&self, start_dir: &Path) -> Option<String> {
        self.load(start_dir).map(|content| {
            format!(
                "The following project instructions were loaded automatically. \
                 Follow them carefully:\n\n{content}"
            )
        })
    }
}
