/// A named section of the system prompt.
#[derive(Debug, Clone)]
pub struct PromptSection {
    pub name: String,
    pub content: String,
}

impl PromptSection {
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content: content.into(),
        }
    }
}

/// Builds a system prompt from modular sections.
///
/// Separates static (cacheable) and dynamic (per-session) sections,
/// mirroring Claude Code's prompt caching strategy.
pub struct SystemPromptBuilder {
    static_sections: Vec<PromptSection>,
    dynamic_sections: Vec<PromptSection>,
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self {
            static_sections: Vec::new(),
            dynamic_sections: Vec::new(),
        }
    }

    /// Add a static section (stable across sessions, cacheable).
    pub fn static_section(mut self, section: PromptSection) -> Self {
        self.static_sections.push(section);
        self
    }

    /// Add a dynamic section (changes per session).
    pub fn dynamic_section(mut self, section: PromptSection) -> Self {
        self.dynamic_sections.push(section);
        self
    }

    /// Get the static portion (for caching).
    pub fn static_prompt(&self) -> String {
        self.static_sections
            .iter()
            .map(|s| format!("# {}\n{}", s.name, s.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Get the dynamic portion.
    pub fn dynamic_prompt(&self) -> String {
        self.dynamic_sections
            .iter()
            .map(|s| format!("# {}\n{}", s.name, s.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Build the complete system prompt.
    pub fn build(&self) -> String {
        let static_part = self.static_prompt();
        let dynamic_part = self.dynamic_prompt();

        if dynamic_part.is_empty() {
            static_part
        } else if static_part.is_empty() {
            dynamic_part
        } else {
            format!("{static_part}\n\n{dynamic_part}")
        }
    }

    /// Number of sections.
    pub fn section_count(&self) -> usize {
        self.static_sections.len() + self.dynamic_sections.len()
    }
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}
