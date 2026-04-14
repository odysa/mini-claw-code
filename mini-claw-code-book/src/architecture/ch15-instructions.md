# Chapter 15: Project Instructions

In Chapter 5 you built two things that did not yet know about each other. The
`SystemPromptBuilder` assembles a prompt from static and dynamic sections. The
`InstructionLoader` discovers CLAUDE.md files by walking up the filesystem. You
wired a basic example together at the end of that chapter, but nothing in the
codebase actually used that wiring. The instruction loader was a standalone
utility, and the builder was a general-purpose assembler.

In Chapter 14 you added `Config`, a layered settings hierarchy. One of its
fields is `instructions: Option<String>` -- custom text that the user can put
in a TOML config file and have injected into the system prompt.

This chapter connects all three. It is the chapter where your agent becomes
*project-aware* -- where launching the agent from `/home/user/project/backend`
produces a different system prompt than launching it from `/home/user/other`.
The pieces were already built. Now they form a pipeline.

```bash
cargo test -p mini-claw-code-starter test_ch15
```

---

## The instruction pipeline

Here is the complete flow, from files on disk to tokens in the prompt:

```
  ┌─────────────────────────────┐
  │  Filesystem                 │
  │                             │
  │  /home/user/CLAUDE.md       │──┐
  │  /home/user/project/        │  │
  │    CLAUDE.md                │──┤  InstructionLoader::discover()
  │    backend/                 │  │  walks upward, collects paths
  │      CLAUDE.md              │──┤
  │      .claw/instructions.md  │──┘
  └─────────────────────────────┘
              │
              ▼
  ┌─────────────────────────────┐
  │  InstructionLoader::load()  │
  │                             │
  │  Reads each file, skips     │
  │  empty ones, joins with     │
  │  headers and --- separators │
  └─────────────────────────────┘
              │
              ▼
  ┌─────────────────────────────┐
  │  SystemPromptBuilder        │
  │                             │
  │  STATIC:                    │
  │    identity                 │
  │    safety                   │
  │  ──── CACHE BOUNDARY ────── │
  │  DYNAMIC:                   │
  │    environment              │
  │    file_instructions  ◄─────│── from InstructionLoader
  │    config_instructions ◄────│── from Config.instructions
  └─────────────────────────────┘
              │
              ▼
        System prompt string
```

File-based instructions and config-based instructions are both dynamic
sections. They depend on which directory the agent is launched from and which
config files are loaded, both of which change between sessions. Static sections
-- identity, safety, tool schemas -- stay above the cache boundary where they
belong.

---

## Revisiting InstructionLoader

You built this in Chapter 5. Let's revisit the code now that we are using it
in a real pipeline, because the design decisions matter more in context.

### The struct

```rust
pub struct InstructionLoader {
    file_names: Vec<String>,
}
```

The loader does not hardcode which files to look for. It takes a list of file
names, and `default_files()` sets that list to `["CLAUDE.md",
".claw/instructions.md"]`. This means you can swap in different file names
for testing, or add project-specific alternatives without modifying the loader.

```rust
impl InstructionLoader {
    pub fn new(file_names: &[&str]) -> Self {
        Self {
            file_names: file_names.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn default_files() -> Self {
        Self::new(&["CLAUDE.md", ".claw/instructions.md"])
    }
}
```

### Discovery: the upward walk

`discover()` starts at the given directory and walks toward the filesystem
root. At each directory, it checks for every file name in the list:

```rust
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
```

The `found.reverse()` at the end is the key design choice. The walk naturally
collects files from most-specific to most-general (start directory first, root
last). Reversing puts them in root-first order.

After `discover("/home/user/project/backend")` with CLAUDE.md files at three
levels, the vector is:

```
[0] /home/user/CLAUDE.md               ← global preferences
[1] /home/user/project/CLAUDE.md       ← project conventions
[2] /home/user/project/backend/CLAUDE.md ← subdirectory rules
```

Global preferences come first. The most specific rules come last. When the LLM
reads the system prompt, the last instructions have the strongest influence --
the same principle as CSS specificity: general rules first, overrides last.

### Loading: read, filter, join

`load()` calls `discover()`, reads each file, and concatenates the results:

```rust
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
```

Three details:

**Headers.** Each file's content is prefixed with `# Instructions from <path>`.
This tells the LLM where each block came from, helping it resolve
contradictions between levels.

**Separators.** Files are joined with `\n\n---\n\n` -- a horizontal rule in
markdown that gives the LLM a clear boundary between instruction blocks.

**Empty file skipping.** If a CLAUDE.md exists but is empty or whitespace-only,
it is silently skipped. No point wasting context tokens on an empty section.

**Returning `None`.** If no instruction files are found, or all are empty,
`load()` returns `None` rather than `Some("")`. This lets the caller skip
adding an instructions section entirely.

---

## The instruction hierarchy

Instructions can come from multiple sources. Here is the full hierarchy, from
broadest to most specific:

```
Source                              Priority    Section type
──────────────────────────────────────────────────────────────
/home/user/CLAUDE.md                lowest      file (root-first)
/home/user/project/CLAUDE.md        ↓           file
/home/user/project/backend/CLAUDE.md ↓          file
.claw/instructions.md               ↓           file (alternative)
Config.instructions                 highest     config
```

File-based instructions are discovered by the `InstructionLoader` and appear
in root-first order. Config-based instructions come from the `Config` struct's
`instructions` field -- loaded from `.claw/config.toml` or
`~/.config/mini-claw/config.toml`.

Both become dynamic sections in the system prompt. File instructions are added
first, config instructions second. Since the LLM reads the prompt top-to-bottom,
config instructions have the final word when there is a conflict.

### Why two sources?

CLAUDE.md files are committed to version control. They represent team
conventions that everyone on the project shares. "Run tests with `cargo test`."
"Never modify generated files." "Use edition 2024."

Config instructions are local. They live in `.claw/config.toml` (which may or
may not be committed) or in the user's home config directory (which is never
committed). They represent personal preferences or temporary overrides.
"Always explain your reasoning." "Focus on performance over readability for
this session."

---

## Wiring it together

Here is how the three systems -- `InstructionLoader`, `SystemPromptBuilder`,
and `Config` -- combine into a single prompt assembly:

```rust
fn build_prompt(cwd: &str, config: &Config) -> String {
    let mut builder = SystemPromptBuilder::new()
        .static_section(PromptSection::new(
            "identity",
            "You are a coding agent. You help users with software engineering \
             tasks using the tools available to you.",
        ))
        .static_section(PromptSection::new(
            "safety",
            "Be careful not to introduce security vulnerabilities. \
             Prioritize writing safe, secure, and correct code.",
        ))
        .dynamic_section(PromptSection::new(
            "environment",
            format!("Working directory: {cwd}"),
        ));

    // File-based instructions (CLAUDE.md files)
    let loader = InstructionLoader::default_files();
    if let Some(instructions) = loader.load(Path::new(cwd)) {
        builder = builder.dynamic_section(
            PromptSection::new("file_instructions", instructions),
        );
    }

    // Config-based instructions
    if let Some(ref inst) = config.instructions {
        builder = builder.dynamic_section(
            PromptSection::new("config_instructions", inst.clone()),
        );
    }

    builder.build()
}
```

The order of `dynamic_section` calls determines the order in the prompt:

1. Environment info (working directory)
2. File instructions (CLAUDE.md files, root-first)
3. Config instructions (from config.toml)

This is deliberate. Environment context comes first so the LLM knows where it
is working. File instructions provide project conventions. Config instructions
get the last word.

### The cache boundary in practice

A provider that supports prompt caching sends `builder.static_prompt()` with
a cache control header and `builder.dynamic_prompt()` fresh on each request.
For a project with two CLAUDE.md files and config instructions, the prompt
looks like:

```text
# identity                              ─┐
You are a coding agent...                │ static_prompt()
                                         │ (cached)
# safety                                │
Be careful not to introduce...          ─┘

# environment                           ─┐
Working directory: /home/user/project    │
                                         │
# file_instructions                      │ dynamic_prompt()
# Instructions from /home/user/CLAUDE.md │ (fresh each request)
                                         │
Use American English.                    │
                                         │
---                                      │
                                         │
# Instructions from .../project/CLAUDE.md│
                                         │
Run tests with `cargo test`.             │
                                         │
# config_instructions                    │
Always explain your reasoning.          ─┘
```

The static half is processed once and cached. The dynamic half -- which
includes all instructions -- is reprocessed on each API call. This is the
right split. Instructions can change if the user edits a CLAUDE.md file
mid-session, and the agent should pick up those changes.

---

## Section ordering and section count

The `SystemPromptBuilder` tracks the total number of sections across both
lists:

```rust
pub fn section_count(&self) -> usize {
    self.static_sections.len() + self.dynamic_sections.len()
}
```

For a prompt with identity, safety, environment, file instructions, and config
instructions, that is 5 sections: 2 static + 3 dynamic. The test suite
verifies this count to ensure no sections are accidentally dropped during
assembly.

The count is also useful for debugging. If `section_count()` returns 2 when
you expected 5, you know the instruction loading failed to find any files.
The first thing to check when the agent misbehaves is whether the system
prompt contains what you think it contains.

---

## How Claude Code does it

Claude Code discovers CLAUDE.md files by walking up from the working directory,
following the same upward-walk pattern we implemented. But its instruction
system is more elaborate in several ways.

**User-level instructions.** Claude Code supports `~/.claude/CLAUDE.md` as a
global instruction file. Our `InstructionLoader` achieves the same effect
naturally: if the upward walk reaches the home directory and finds a CLAUDE.md,
it gets included. No special case needed.

**Settings-based tool rules.** Claude Code's `.claude/settings.json` specifies
per-tool permission rules. These configure the permission engine (Chapter 10),
not the prompt. Our `Config` keeps it simpler with `allowed_directory`,
`protected_patterns`, and `blocked_commands`.

**Memory files.** Claude Code supports persistent memory that accumulates facts
across sessions. Memory is loaded alongside instructions but managed separately.
We will build a simpler version in Chapter 16.

**Instruction validation.** Claude Code warns when instructions at different
levels contradict each other. Our implementation trusts the LLM to resolve
contradictions using the root-first ordering -- the more specific instruction
wins because it appears later.

The core pattern is identical: discover files, load them in order, inject as
dynamic prompt sections. Everything else is refinement.

---

## Tests

Run the chapter 15 tests:

```bash
cargo test -p mini-claw-code-starter test_ch15
```

The tests are organized in three groups: discovery, loading, and integration.

### Discovery tests

- **`test_ch15_discover_single_file`** -- Creates a temp directory with a
  single CLAUDE.md file. Calls `discover()` and verifies it finds exactly one
  path, and that path matches the file we created.

- **`test_ch15_discover_nested_hierarchy`** -- Creates a root directory with a
  CLAUDE.md and a nested `project/backend` subdirectory with its own CLAUDE.md.
  Calls `discover()` from the backend directory. Verifies both files are found
  and the root-level file comes before the subdirectory file in the result
  vector. This is the root-first ordering test.

- **`test_ch15_discover_no_files`** -- Creates an empty temp directory. Calls
  `discover()` and verifies it either finds no files or only finds files
  outside the temp directory (from parent directories on the actual
  filesystem). The key assertion is that it does not panic.

- **`test_ch15_discover_custom_file_names`** -- Creates a RULES.md file and
  constructs an `InstructionLoader` with `["RULES.md"]` instead of the default
  file names. Verifies `discover()` finds the custom file. This tests the
  parameterization -- the loader is not hardcoded to CLAUDE.md.

- **`test_ch15_discover_claw_instructions`** -- Creates a `.claw/instructions.md`
  file in the temp directory. Uses `default_files()` and verifies `discover()`
  finds the file. This tests the alternative instruction file location.

### Loading tests

- **`test_ch15_load_single_file`** -- Creates a CLAUDE.md with content. Calls
  `load()` and verifies the result contains the file's content and the
  `"Instructions from"` header prefix.

- **`test_ch15_load_multiple_files`** -- Creates a root CLAUDE.md and a
  subdirectory CLAUDE.md. Calls `load()` from the subdirectory. Verifies both
  files' content appears in the result and the `---` separator is present
  between them.

- **`test_ch15_load_empty_files_skipped`** -- Creates an empty CLAUDE.md. Calls
  `load()` and verifies it returns `None`. Empty files should not produce empty
  sections in the prompt.

- **`test_ch15_load_no_files_returns_none`** -- Calls `load()` in a temp
  directory with no instruction files. Verifies it does not panic. (It may
  return `None` or `Some` depending on whether the temp directory's parent
  chain contains any CLAUDE.md files.)

### Integration tests

- **`test_ch15_instructions_in_system_prompt`** -- The end-to-end test. Creates
  a CLAUDE.md, loads it with `InstructionLoader`, and injects it as a dynamic
  section into a `SystemPromptBuilder` alongside static identity and safety
  sections. Calls `build()` and verifies the final prompt contains text from
  all three sections. This proves the pipeline works from file to prompt.

- **`test_ch15_static_dynamic_separation`** -- Creates a CLAUDE.md and builds
  a prompt with one static section (identity) and one dynamic section
  (instructions). Verifies that `static_prompt()` contains the identity text
  but not the instructions, `dynamic_prompt()` contains the instructions but
  not the identity text, and `build()` contains both. This is the cache
  boundary test -- it ensures instructions are always on the dynamic side.

- **`test_ch15_config_instructions_override`** -- Adds both file-based
  instructions and config-based instructions as separate dynamic sections.
  Verifies the final prompt contains both. The config section appears after the
  file section, giving it higher effective priority.

- **`test_ch15_section_count`** -- Builds a prompt with 2 static sections and
  1 dynamic section. Verifies `section_count()` returns 3. A simple bookkeeping
  check that catches off-by-one errors in the builder.

---

## Recap

This chapter connected three systems that were built independently:

- **`InstructionLoader`** discovers CLAUDE.md files by walking up the
  filesystem and loads them into a single string with headers and separators.
  Files are ordered root-first so that global preferences appear before
  project-specific rules.

- **`SystemPromptBuilder`** separates static sections (identity, safety) from
  dynamic sections (environment, instructions). The static half is cacheable.
  The dynamic half is fresh each request.

- **`Config.instructions`** provides an additional source of instructions from
  the config hierarchy. Config instructions are added as the last dynamic
  section, giving them the highest effective priority.

The pipeline is: discover files on disk, load and concatenate them, inject as
a dynamic section, optionally add config instructions as a second dynamic
section, build the final prompt. The result is a system prompt that adapts to
whichever directory the agent is launched from.

The key insight is that **instructions are always dynamic**. Even though
CLAUDE.md files might not change often, they depend on the working directory
-- launching from a different location discovers different files. Keeping them
below the cache boundary ensures the agent always uses the correct instructions
for the current session, while the stable parts of the prompt (identity, safety,
tool schemas) stay cached.

---

## What's next

In [Chapter 16: Memory](./ch16-memory.md) you will add persistent memory --
facts that the agent learns during one session and remembers in the next.
Memory files are loaded alongside instructions, but they are managed differently:
instructions are authored by humans, memory is authored by the agent itself.
