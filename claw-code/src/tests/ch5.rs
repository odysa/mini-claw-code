use crate::prompt::*;

#[test]
fn test_ch5_builder_empty() {
    let builder = SystemPromptBuilder::new();
    assert_eq!(builder.section_count(), 0);
    assert_eq!(builder.build(), "");
}

#[test]
fn test_ch5_static_section() {
    let prompt = SystemPromptBuilder::new()
        .static_section(PromptSection::new("identity", "You are helpful."))
        .build();
    assert!(prompt.contains("identity"));
    assert!(prompt.contains("You are helpful."));
}

#[test]
fn test_ch5_dynamic_section() {
    let prompt = SystemPromptBuilder::new()
        .dynamic_section(PromptSection::new("env", "cwd: /tmp"))
        .build();
    assert!(prompt.contains("cwd: /tmp"));
}

#[test]
fn test_ch5_static_and_dynamic() {
    let prompt = SystemPromptBuilder::new()
        .static_section(PromptSection::new("core", "Be helpful."))
        .dynamic_section(PromptSection::new("env", "cwd: /tmp"))
        .build();
    assert!(prompt.contains("Be helpful."));
    assert!(prompt.contains("cwd: /tmp"));
    // Static comes before dynamic
    let static_pos = prompt.find("Be helpful.").unwrap();
    let dynamic_pos = prompt.find("cwd: /tmp").unwrap();
    assert!(static_pos < dynamic_pos);
}

#[test]
fn test_ch5_multiple_sections() {
    let prompt = SystemPromptBuilder::new()
        .static_section(PromptSection::new("a", "Section A"))
        .static_section(PromptSection::new("b", "Section B"))
        .dynamic_section(PromptSection::new("c", "Section C"))
        .build();
    assert!(prompt.contains("Section A"));
    assert!(prompt.contains("Section B"));
    assert!(prompt.contains("Section C"));
}

#[test]
fn test_ch5_section_count() {
    let builder = SystemPromptBuilder::new()
        .static_section(PromptSection::new("a", ""))
        .static_section(PromptSection::new("b", ""))
        .dynamic_section(PromptSection::new("c", ""));
    assert_eq!(builder.section_count(), 3);
}

#[test]
fn test_ch5_static_prompt_only() {
    let builder = SystemPromptBuilder::new().static_section(PromptSection::new("core", "Be safe."));
    let static_part = builder.static_prompt();
    let dynamic_part = builder.dynamic_prompt();
    assert!(static_part.contains("Be safe."));
    assert!(dynamic_part.is_empty());
}

#[test]
fn test_ch5_default_system_prompt() {
    let prompt = build_default_system_prompt("/home/user/project");
    assert!(prompt.contains("coding agent"));
    assert!(prompt.contains("/home/user/project"));
}

// --- InstructionLoader ---

#[test]
fn test_ch5_instruction_loader_discover() {
    use crate::prompt::instructions::InstructionLoader;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# Project\nBe concise.").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md"]);
    let found = loader.discover(dir.path());
    assert!(!found.is_empty());
}

#[test]
fn test_ch5_instruction_loader_load() {
    use crate::prompt::instructions::InstructionLoader;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "Be concise.").unwrap();

    let loader = InstructionLoader::new(&["CLAUDE.md"]);
    let content = loader.load(dir.path()).unwrap();
    assert!(content.contains("Be concise."));
}

#[test]
fn test_ch5_instruction_loader_no_files() {
    use crate::prompt::instructions::InstructionLoader;

    let dir = tempfile::tempdir().unwrap();
    let loader = InstructionLoader::new(&["NONEXISTENT.md"]);
    assert!(loader.load(dir.path()).is_none());
}
