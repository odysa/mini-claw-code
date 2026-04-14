use crate::instructions::InstructionLoader;

#[test]
fn test_ch17_discover_in_current_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# Instructions\nBe helpful.").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md"]);
    let found = loader.discover(dir.path());
    assert_eq!(found.len(), 1);
    assert!(found[0].ends_with("CLAUDE.md"));
}

#[test]
fn test_ch17_discover_in_parent() {
    let parent = tempfile::tempdir().unwrap();
    let child = parent.path().join("subdir");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(parent.path().join("CLAUDE.md"), "Parent instructions").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md"]);
    let found = loader.discover(&child);
    // Should find the parent's CLAUDE.md
    assert!(!found.is_empty());
    assert!(found.iter().any(|p| p.ends_with("CLAUDE.md")));
}

#[test]
fn test_ch17_no_files_found() {
    let dir = tempfile::tempdir().unwrap();
    let loader = InstructionLoader::new(&["NONEXISTENT.md"]);
    let found = loader.discover(dir.path());
    assert!(found.is_empty());
}

#[test]
fn test_ch17_load_content() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "Be concise.").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md"]);
    let content = loader.load(dir.path());
    assert!(content.is_some());
    assert!(content.unwrap().contains("Be concise."));
}

#[test]
fn test_ch17_load_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md"]);
    let content = loader.load(dir.path());
    assert!(content.is_none());
}

#[test]
fn test_ch17_multiple_file_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "Primary").unwrap();

    let sub = dir.path().join(".mini-claw");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("instructions.md"), "Secondary").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md", ".mini-claw/instructions.md"]);
    let content = loader.load(dir.path());
    assert!(content.is_some());
    let text = content.unwrap();
    assert!(text.contains("Primary"));
    assert!(text.contains("Secondary"));
}

#[test]
fn test_ch17_system_prompt_section() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "Use Rust idioms.").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md"]);
    let section = loader.system_prompt_section(dir.path());
    assert!(section.is_some());
    let text = section.unwrap();
    assert!(text.contains("project instructions"));
    assert!(text.contains("Use Rust idioms."));
}

#[test]
fn test_ch17_default_files() {
    let loader = InstructionLoader::default_files();
    // Should not panic
    let _ = loader.discover(std::path::Path::new("/tmp"));
}
