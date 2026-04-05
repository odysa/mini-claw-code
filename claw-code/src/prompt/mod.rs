pub mod instructions;
mod sections;

pub use sections::{PromptSection, SystemPromptBuilder};

/// Build a complete system prompt from modular sections.
///
/// Mirrors Claude Code's system prompt assembly: static sections
/// (tool schemas, core instructions) are separated from dynamic
/// sections (git status, CLAUDE.md, user context) by a cache boundary.
///
/// ```text
/// ┌─────────────────────────────────────┐
/// │ Static sections (cacheable)         │
/// │  - Tool schemas                     │
/// │  - Core instructions                │
/// │  - Safety instructions              │
/// ├─────── CACHE BOUNDARY ──────────────┤
/// │ Dynamic sections (per-session)      │
/// │  - Working directory                │
/// │  - Git status                       │
/// │  - CLAUDE.md instructions           │
/// │  - Custom instructions              │
/// └─────────────────────────────────────┘
/// ```
pub fn build_default_system_prompt(cwd: &str) -> String {
    SystemPromptBuilder::new()
        .static_section(PromptSection::new(
            "identity",
            "You are a coding agent. You help users with software engineering tasks \
             using the tools available to you.",
        ))
        .static_section(PromptSection::new(
            "safety",
            "Be careful not to introduce security vulnerabilities. \
             Prioritize writing safe, secure, and correct code.",
        ))
        .dynamic_section(PromptSection::new(
            "environment",
            format!("Working directory: {cwd}"),
        ))
        .build()
}
