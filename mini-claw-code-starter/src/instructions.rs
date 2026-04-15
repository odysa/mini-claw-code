use std::path::{Path, PathBuf};

/// Discovers and loads project instruction files (like CLAUDE.md).
pub struct InstructionLoader {
    file_names: Vec<String>,
}

impl InstructionLoader {
    pub fn new(file_names: &[&str]) -> Self {
        unimplemented!("Convert file_names to Vec<String>")
    }

    /// Create a loader with defaults: CLAUDE.md, .mini-claw/instructions.md
    pub fn default_files() -> Self {
        Self::new(&["CLAUDE.md", ".mini-claw/instructions.md"])
    }

    /// Discover instruction files by walking up from start_dir.
    pub fn discover(&self, start_dir: &Path) -> Vec<PathBuf> {
        unimplemented!("Walk up from start_dir, collect matching files, reverse")
    }

    /// Load and concatenate all discovered files, separated by headers.
    pub fn load(&self, start_dir: &Path) -> Option<String> {
        unimplemented!("Discover files, read each, join with headers")
    }

    /// Build a system prompt section from discovered instructions.
    pub fn system_prompt_section(&self, start_dir: &Path) -> Option<String> {
        unimplemented!("Call load(), wrap with instruction preamble")
    }
}
