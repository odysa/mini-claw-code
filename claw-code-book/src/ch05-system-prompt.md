# Chapter 5: System Prompt

Every LLM-based agent starts with a system prompt -- an invisible preamble that
shapes every response the model produces. A sloppy prompt gives you a chatbot.
A carefully engineered prompt gives you a coding agent that follows safety rules,
uses tools correctly, and adapts to the project it is working in.

Claude Code's system prompt is over 900 lines of assembled text. It is not
written as a single string. It is built from **modular sections** -- identity,
safety rules, tool schemas, environment info, project instructions -- stitched
together by a builder at startup. Some sections never change between sessions
(tool schemas, core instructions). Others change every time (working directory,
git status, CLAUDE.md contents). This distinction is not cosmetic. It is the
foundation of **prompt caching**, an optimization that can cut costs and latency
dramatically.

In this chapter you will build the system prompt infrastructure: a section type,
a builder that separates static from dynamic content, an instruction loader that
discovers project-specific CLAUDE.md files, and a default prompt assembler that
wires it all together.

## Goal

Implement the prompt module so that:

1. `PromptSection` holds a named chunk of prompt text.
2. `SystemPromptBuilder` collects static and dynamic sections, renders them
   separately or combined.
3. `InstructionLoader` walks up the filesystem to discover and load CLAUDE.md
   files.
4. `build_default_system_prompt()` assembles a minimal but complete prompt.

## Why system prompts matter for agents

A vanilla LLM is a text completer. It has no idea it can run bash commands, read
files, or edit code -- unless you tell it. The system prompt is where you tell it.

For a coding agent, the system prompt must do several things:

- **Identity**: "You are a coding agent with access to tools." Without this, the
  model may refuse tool calls or behave like a generic assistant.
- **Safety**: "Do not delete files outside the working directory. Do not
  introduce security vulnerabilities." Safety rules constrain what the model
  will attempt.
- **Tool schemas**: The JSON schema definitions for every available tool. The
  model needs these to know *how* to call tools -- what parameters they accept,
  which are required, what types they expect.
- **Environment**: The working directory, OS, shell, git status. This context
  prevents the model from guessing about the environment.
- **Project instructions**: Contents of CLAUDE.md files that tell the model
  about project conventions, preferred patterns, and things to avoid.

Claude Code assembles all of these into a single system prompt before each
conversation. Sections are ordered deliberately, and a cache boundary separates
the parts that change from the parts that do not.

## The section architecture

Open `src/prompt/sections.rs`. The smallest unit of the prompt
is a `PromptSection` -- a named chunk of text:

```rust
/// A named section of the system prompt.
#[derive(Debug, Clone)]
pub struct PromptSection {
    pub name: String,
    pub content: String,
}
```

The `name` field serves as a heading when the section is rendered. The `content`
field holds the actual prompt text. Each section renders as:

```text
# identity
You are a coding agent. You help users with software engineering tasks
using the tools available to you.
```

The heading helps the LLM parse the prompt structure and makes debugging easier
when you inspect the assembled prompt.

### Implementing `PromptSection`

The constructor accepts anything that converts to `String`, using
`impl Into<String>`:

```rust
impl PromptSection {
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content: content.into(),
        }
    }
}
```

This lets callers pass `&str`, `String`, or `format!(...)` output without
friction.

## The builder: static vs. dynamic sections

`SystemPromptBuilder` is where the cache boundary concept lives. It maintains
two separate lists of sections:

```rust
pub struct SystemPromptBuilder {
    static_sections: Vec<PromptSection>,
    dynamic_sections: Vec<PromptSection>,
}
```

### Why two lists?

LLM API calls are expensive. Every token in the system prompt is processed on
every request. Claude's prompt caching feature lets you mark a prefix of the
prompt as cacheable -- the API processes it once, caches the internal state, and
reuses it on subsequent requests. This can reduce latency by up to 85% and cost
by up to 90% for long prompts.

But caching only works for a **prefix**. If any byte in the cached prefix
changes, the cache is invalidated. This means you need to put the stable parts
first and the changing parts last:

```text
+---------------------------------------+
| Static sections (cacheable)           |
|  - Identity                           |
|  - Safety instructions                |
|  - Tool schemas                       |
|                                       |
|  [these rarely change]                |
+-------- CACHE BOUNDARY ---------------+
| Dynamic sections (per-session)        |
|  - Working directory                  |
|  - Git status                         |
|  - CLAUDE.md instructions             |
|  - Custom user instructions           |
|                                       |
|  [these change every session]         |
+---------------------------------------+
```

Claude Code calls this boundary `SYSTEM_PROMPT_DYNAMIC_BOUNDARY`. Everything
above it is sent with a cache control header. Everything below it is fresh on
each request. Our builder encodes this same separation structurally.

### Implementing the builder

The builder uses a fluent API. Each method takes `self` and returns `Self`:

```rust
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
}
```

This lets you chain calls to build up the prompt:

```rust
SystemPromptBuilder::new()
    .static_section(PromptSection::new("identity", "You are a coding agent."))
    .static_section(PromptSection::new("safety", "Be careful."))
    .dynamic_section(PromptSection::new("env", "cwd: /home/user/project"))
```

### Rendering methods

The builder exposes three rendering methods:

- **`static_prompt()`** -- renders only the static sections. Send this half with
  a cache control header.
- **`dynamic_prompt()`** -- renders only the dynamic sections. Send this half
  fresh each request.
- **`build()`** -- concatenates both halves into a single string. Use this when
  the provider does not support prompt caching.

Each method formats sections the same way: `# name` heading, then content,
separated by blank lines. Implement all three -- `static_prompt()` and
`dynamic_prompt()` iterate their respective lists, and `build()` joins their
outputs with empty-string checks to avoid stray blank lines.

Also implement `section_count()` (returns the total across both lists) and the
`Default` trait (delegates to `new()`).

## The default system prompt

With the builder in place, assemble a minimal default prompt in
`src/prompt/mod.rs`. The function `build_default_system_prompt`
takes a working directory string and wires up three sections:

1. **Identity** (static) -- tells the model it is a coding agent with tools.
2. **Safety** (static) -- tells the model to prioritize secure code.
3. **Environment** (dynamic) -- tells the model the current working directory.

Identity and safety never change between sessions, so they are static. The
working directory changes every time, so it is dynamic. A production agent would
add tool schemas (static) and git status, OS info, and CLAUDE.md contents
(dynamic). The builder makes it trivial to add more sections later.

## InstructionLoader: discovering CLAUDE.md

Claude Code loads project-specific instructions from CLAUDE.md files. These
files let users customize the agent's behavior per project -- preferred coding
style, test commands, things to avoid. The agent discovers them by walking up
the filesystem from the current working directory.

Open `src/prompt/instructions.rs`.

### The struct

```rust
pub struct InstructionLoader {
    file_names: Vec<String>,
}
```

The loader is parameterized by file names to search for. The default
configuration looks for `CLAUDE.md` and `.claw/instructions.md`:

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

### `discover()` -- walking upward

The `discover()` method starts at a given directory and walks toward the
filesystem root, checking each directory for the target files:

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

The walk collects files from the start directory up to the root, then reverses
the list so root-level files come first. This ordering matters: global
instructions appear before project-specific ones, and the LLM sees the most
specific instructions last (closest to the user prompt).

Consider a project at `/home/user/project/backend`:

```text
/home/user/CLAUDE.md                  <-- global preferences
/home/user/project/CLAUDE.md          <-- project conventions
/home/user/project/backend/CLAUDE.md  <-- backend-specific rules
```

After `discover()`, the vector contains them in that order: global first, most
specific last.

### `load()` -- reading and concatenating

The `load()` method calls `discover()`, reads each file, and joins them into a
single string. Each file's content is prefixed with `# Instructions from <path>`
so the LLM knows where each block came from. Files are separated by `---`
markers. Empty or unreadable files are silently skipped. If no instruction files
exist at all, `load()` returns `None`.

The output for two files looks like:

```text
# Instructions from /home/user/CLAUDE.md

Use American English. Prefer explicit error handling.

---

# Instructions from /home/user/project/CLAUDE.md

Run tests with `cargo test`. Never modify generated files.
```

## The full assembly flow

In a production agent, the builder and instruction loader combine into a
pipeline. Steps 1-3 are static (identical across sessions); steps 4-5 are
dynamic (recomputed each time):

```text
  STATIC:   identity -> safety -> tool schemas
                  ---- CACHE BOUNDARY ----
  DYNAMIC:  environment -> CLAUDE.md instructions
```

To wire the instruction loader into the builder:

```rust
let mut builder = SystemPromptBuilder::new()
    .static_section(PromptSection::new("identity", "..."))
    .static_section(PromptSection::new("safety", "..."))
    .dynamic_section(PromptSection::new("environment", format!("cwd: {cwd}")));

let loader = InstructionLoader::default_files();
if let Some(instructions) = loader.load(Path::new(cwd)) {
    builder = builder.dynamic_section(
        PromptSection::new("project_instructions", instructions)
    );
}

let prompt = builder.build();
```

The instructions are always dynamic -- they depend on which directory the agent
is launched from.

## How Claude Code does it

Claude Code's prompt assembly follows the same principles at larger scale. Its
system prompt includes identity, safety rules, tool schemas, behavioral
guidelines, environment details, CLAUDE.md instructions from multiple levels,
and session metadata -- routinely exceeding 900 lines.

Without prompt caching, every API call would reprocess all of that. Claude Code
marks the cache boundary with a `SYSTEM_PROMPT_DYNAMIC_BOUNDARY` marker. The
provider splits the system message at this boundary and sends the prefix with
`cache_control: { type: "ephemeral" }`. The API caches the prefix's internal
representation and reuses it for subsequent requests, often covering 80%+ of
the prompt.

Our `SystemPromptBuilder` achieves the same split structurally. The
`static_prompt()` and `dynamic_prompt()` methods give you the two halves. A
provider that supports caching sends `static_prompt()` with cache control and
`dynamic_prompt()` without it.

## Running the tests

Run the Chapter 5 tests:

```bash
cargo test -p claw-code test_ch5
```

### What the tests verify

- **`test_ch5_builder_empty`**: A fresh builder has zero sections and builds to
  an empty string.
- **`test_ch5_static_section`**: A builder with one static section renders the
  section name and content.
- **`test_ch5_dynamic_section`**: Same for a dynamic section.
- **`test_ch5_static_and_dynamic`**: Both types of sections are present. The
  static section appears before the dynamic section in the output.
- **`test_ch5_multiple_sections`**: Three sections (two static, one dynamic) all
  appear in the output.
- **`test_ch5_section_count`**: Verifies `section_count()` returns the total
  across both lists.
- **`test_ch5_static_prompt_only`**: `static_prompt()` returns content,
  `dynamic_prompt()` returns empty when no dynamic sections exist.
- **`test_ch5_default_system_prompt`**: `build_default_system_prompt()` includes
  the identity text and the working directory.
- **`test_ch5_instruction_loader_discover`**: Creates a temp directory with a
  CLAUDE.md file and verifies `discover()` finds it.
- **`test_ch5_instruction_loader_load`**: Same setup, verifies `load()` returns
  the file's content.
- **`test_ch5_instruction_loader_no_files`**: No instruction files exist.
  `load()` returns `None`.

## Recap

You have built the system prompt infrastructure:

- **`PromptSection`** is a named chunk of prompt text -- the atom of prompt
  assembly.
- **`SystemPromptBuilder`** separates static (cacheable) sections from dynamic
  (per-session) sections. It can render each half independently for prompt
  caching or combine them with `build()`.
- **`InstructionLoader`** discovers CLAUDE.md files by walking up the filesystem.
  It concatenates them in root-first order so that global instructions appear
  before project-specific ones.
- **`build_default_system_prompt()`** assembles a minimal prompt with identity,
  safety, and environment sections.

The key insight is the **cache boundary**. By separating what changes from what
does not, you enable prompt caching -- a single optimization that can cut costs
and latency by an order of magnitude on long prompts. Every production agent
does this. Now yours does too.

## What's next

In [Chapter 6: File Tools](./ch06-file-tools.md) you will implement the tools
that let your agent interact with the filesystem -- reading, writing, and
editing files. These are the tools whose schemas will eventually appear in the
static portion of your system prompt.
