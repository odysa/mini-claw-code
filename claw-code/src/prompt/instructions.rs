use std::path::{Path, PathBuf};

/// Discovers and loads project instruction files (CLAUDE.md).
///
/// Walks from the given directory upward to the filesystem root,
/// collecting instruction files. Files closer to the root are
/// loaded first; subdirectory files appear later.
pub struct InstructionLoader {
    file_names: Vec<String>,
}

impl InstructionLoader {
    pub fn new(file_names: &[&str]) -> Self {
        Self {
            file_names: file_names.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn default_files() -> Self {
        Self::new(&["CLAUDE.md", ".claw/instructions.md"])
    }

    /// Discover instruction files walking upward from `start_dir`.
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

        found.reverse(); // Root-first order
        found
    }

    /// Load and concatenate all discovered instruction files.
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
}
