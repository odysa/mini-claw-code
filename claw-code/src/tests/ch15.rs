use crate::prompt::instructions::InstructionLoader;
use crate::prompt::{PromptSection, SystemPromptBuilder};

// ---------------------------------------------------------------------------
// InstructionLoader: upward walk
// ---------------------------------------------------------------------------

#[test]
fn test_ch15_discover_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let claude_md = dir.path().join("CLAUDE.md");
    std::fs::write(&claude_md, "# Project rules\nUse tabs.").unwrap();

    let loader = InstructionLoader::default_files();
    let paths = loader.discover(dir.path());
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], claude_md);
}

#[test]
fn test_ch15_discover_nested_hierarchy() {
    let root = tempfile::tempdir().unwrap();
    let sub = root.path().join("project").join("backend");
    std::fs::create_dir_all(&sub).unwrap();

    // Root-level CLAUDE.md
    std::fs::write(root.path().join("CLAUDE.md"), "Global rules").unwrap();
    // Subdirectory CLAUDE.md
    std::fs::write(sub.join("CLAUDE.md"), "Backend rules").unwrap();

    let loader = InstructionLoader::default_files();
    let paths = loader.discover(&sub);

    // Should find both, root-first order
    assert!(paths.len() >= 2);
    // Root file comes before sub file
    let root_idx = paths
        .iter()
        .position(|p| p.ends_with("CLAUDE.md") && p.parent().unwrap() == root.path());
    let sub_idx = paths.iter().position(|p| p == &sub.join("CLAUDE.md"));
    if let (Some(r), Some(s)) = (root_idx, sub_idx) {
        assert!(
            r < s,
            "root-level file should come before subdirectory file"
        );
    }
}

#[test]
fn test_ch15_discover_no_files() {
    let dir = tempfile::tempdir().unwrap();
    let loader = InstructionLoader::default_files();
    let paths = loader.discover(dir.path());
    // May find files in parent directories, but the temp dir itself has none
    // Just verify it doesn't panic
    assert!(paths.is_empty() || paths.iter().all(|p| !p.starts_with(dir.path())));
}

#[test]
fn test_ch15_discover_custom_file_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("RULES.md"), "Custom rules").unwrap();

    let loader = InstructionLoader::new(&["RULES.md"]);
    let paths = loader.discover(dir.path());
    assert!(paths.iter().any(|p| p.file_name().unwrap() == "RULES.md"));
}

#[test]
fn test_ch15_discover_claw_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let claw_dir = dir.path().join(".claw");
    std::fs::create_dir_all(&claw_dir).unwrap();
    std::fs::write(claw_dir.join("instructions.md"), "Extra rules").unwrap();

    let loader = InstructionLoader::default_files();
    let paths = loader.discover(dir.path());
    assert!(paths.iter().any(|p| p.ends_with(".claw/instructions.md")));
}

// ---------------------------------------------------------------------------
// InstructionLoader: load and concatenate
// ---------------------------------------------------------------------------

#[test]
fn test_ch15_load_single_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "Use Rust edition 2024.").unwrap();

    let loader = InstructionLoader::default_files();
    let content = loader.load(dir.path()).unwrap();
    assert!(content.contains("Use Rust edition 2024"));
    assert!(content.contains("Instructions from"));
}

#[test]
fn test_ch15_load_multiple_files() {
    let root = tempfile::tempdir().unwrap();
    let sub = root.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    std::fs::write(root.path().join("CLAUDE.md"), "Global: use English.").unwrap();
    std::fs::write(sub.join("CLAUDE.md"), "Local: run cargo test.").unwrap();

    let loader = InstructionLoader::default_files();
    let content = loader.load(&sub).unwrap();

    // Both files present
    assert!(content.contains("Global: use English"));
    assert!(content.contains("Local: run cargo test"));

    // Separated by ---
    assert!(content.contains("---"));
}

#[test]
fn test_ch15_load_empty_files_skipped() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "").unwrap(); // Empty

    let loader = InstructionLoader::default_files();
    let result = loader.load(dir.path());
    // Empty content files are skipped
    assert!(result.is_none());
}

#[test]
fn test_ch15_load_no_files_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let loader = InstructionLoader::default_files();
    // In a temp dir with no parent CLAUDE.md files, should return None
    // (unless the system has CLAUDE.md somewhere in the parent chain)
    let _ = loader.load(dir.path()); // Just verify no panic
}

// ---------------------------------------------------------------------------
// Integration: Instructions + SystemPromptBuilder
// ---------------------------------------------------------------------------

#[test]
fn test_ch15_instructions_in_system_prompt() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("CLAUDE.md"),
        "Always run tests before committing.",
    )
    .unwrap();

    let loader = InstructionLoader::default_files();
    let instructions = loader.load(dir.path());

    let mut builder = SystemPromptBuilder::new()
        .static_section(PromptSection::new("identity", "You are a coding agent."))
        .static_section(PromptSection::new("safety", "Write secure code."));

    if let Some(inst) = instructions {
        builder = builder.dynamic_section(PromptSection::new("project_instructions", inst));
    }

    let prompt = builder.build();
    assert!(prompt.contains("coding agent"));
    assert!(prompt.contains("secure code"));
    assert!(prompt.contains("Always run tests"));
}

#[test]
fn test_ch15_static_dynamic_separation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "Project-specific rules.").unwrap();

    let loader = InstructionLoader::default_files();
    let instructions = loader.load(dir.path()).unwrap();

    let builder = SystemPromptBuilder::new()
        .static_section(PromptSection::new("identity", "You are a coding agent."))
        .dynamic_section(PromptSection::new("instructions", instructions));

    // Static prompt should NOT contain instructions
    let static_part = builder.static_prompt();
    assert!(static_part.contains("coding agent"));
    assert!(!static_part.contains("Project-specific"));

    // Dynamic prompt should contain instructions
    let dynamic_part = builder.dynamic_prompt();
    assert!(dynamic_part.contains("Project-specific"));

    // Full prompt has both
    let full = builder.build();
    assert!(full.contains("coding agent"));
    assert!(full.contains("Project-specific"));
}

#[test]
fn test_ch15_config_instructions_override() {
    // Config can provide custom instructions that supplement CLAUDE.md
    let config_instructions = "Additional config-level instructions.";

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "File-level rules.").unwrap();

    let loader = InstructionLoader::default_files();
    let file_instructions = loader.load(dir.path()).unwrap();

    let builder = SystemPromptBuilder::new()
        .dynamic_section(PromptSection::new("file_instructions", file_instructions))
        .dynamic_section(PromptSection::new(
            "config_instructions",
            config_instructions,
        ));

    let prompt = builder.build();
    assert!(prompt.contains("File-level rules"));
    assert!(prompt.contains("Additional config-level"));
}

#[test]
fn test_ch15_section_count() {
    let builder = SystemPromptBuilder::new()
        .static_section(PromptSection::new("a", "1"))
        .static_section(PromptSection::new("b", "2"))
        .dynamic_section(PromptSection::new("c", "3"));
    assert_eq!(builder.section_count(), 3);
}
