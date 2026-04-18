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
    ///
    /// Hints:
    /// - Start at `start_dir`, walk up with `.parent()` until `None`.
    /// - At each level, check every `file_names` entry; push matches.
    /// - Reverse at the end so root-level files come first.
    pub fn discover(&self, _start_dir: &Path) -> Vec<PathBuf> {
        unimplemented!(
            "TODO ch15: walk upward from start_dir collecting matching files, return root-first"
        )
    }

    /// Load and concatenate all discovered files, separated by headers.
    ///
    /// Hints:
    /// - Call `self.discover(start_dir)`.
    /// - For each path, read_to_string; skip if empty after trim.
    /// - Format each section: "# Instructions from {path}\n\n{content}".
    /// - Join sections with "\n\n---\n\n". Return `None` if nothing loaded.
    pub fn load(&self, _start_dir: &Path) -> Option<String> {
        unimplemented!("TODO ch15: read each discovered file and concatenate with headers")
    }

    /// Build a system prompt section from discovered instructions.
    ///
    /// Hint: Call `self.load(start_dir)` and wrap the result in a short preamble
    /// telling the model these are auto-loaded project instructions.
    pub fn system_prompt_section(&self, _start_dir: &Path) -> Option<String> {
        unimplemented!(
            "TODO ch5: wrap the loaded instruction text with a preamble for the system prompt"
        )
    }
}
