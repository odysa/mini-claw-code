# Chapter 5: System Prompt

> **File(s) to edit:** `src/instructions.rs`
> **Test to run:** `cargo test -p mini-claw-code-starter test_ch17` (InstructionLoader)

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

In this chapter you will build the `InstructionLoader` -- the component that
discovers project-specific CLAUDE.md files by walking up the filesystem. We will
also discuss system prompt architecture concepts (sections, static/dynamic
splitting, prompt caching) that production agents like Claude Code use. Our
starter focuses on the instruction loading piece, which is the most practically
useful part.

## Goal

Implement `InstructionLoader` in `src/instructions.rs` so that:

1. `InstructionLoader` walks up the filesystem to discover and load CLAUDE.md
   files.
2. `load()` concatenates discovered files into a single string with headers.
3. `system_prompt_section()` wraps the loaded instructions for inclusion in a
   system prompt.

## How instruction loading works

```mermaid
flowchart TD
    A[InstructionLoader::discover] -->|walks upward| B[/home/user/CLAUDE.md]
    A -->|walks upward| C[/home/user/project/CLAUDE.md]
    A -->|starts here| D[/home/user/project/backend/CLAUDE.md]
    B --> E[Reverse to root-first order]
    C --> E
    D --> E
    E --> F[InstructionLoader::load]
    F -->|concatenates with headers| G[Combined instructions string]
    G --> H[system_prompt_section]
    H --> I[Ready for system prompt]
```

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

## Concepts: sections and cache boundaries

Before diving into the code, let's understand how production agents like Claude
Code structure their system prompts. These concepts inform the design even though
our starter takes a simpler approach.

### Prompt sections

A production system prompt is built from **modular sections** -- identity,
safety rules, tool schemas, environment info, project instructions. Each section
is a named chunk of text that renders as:

```text
# identity
You are a coding agent. You help users with software engineering tasks
using the tools available to you.
```

The heading helps the LLM parse the prompt structure and makes debugging easier
when you inspect the assembled prompt.

### Static vs. dynamic: the cache boundary

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
each request.

A production agent would implement a `SystemPromptBuilder` that maintains
separate lists of static and dynamic sections, renders each half independently,
and supports cache-aware providers. Our starter keeps things simpler -- we
focus on the instruction loading piece, which is the most useful component to
build from scratch.

## InstructionLoader: discovering CLAUDE.md

Claude Code loads project-specific instructions from CLAUDE.md files. These
files let users customize the agent's behavior per project -- preferred coding
style, test commands, things to avoid. The agent discovers them by walking up
the filesystem from the current working directory.

Open `src/instructions.rs`. Here is the starter stub:

```rust
pub struct InstructionLoader {
    file_names: Vec<String>,
}

impl InstructionLoader {
    pub fn new(file_names: &[&str]) -> Self {
        unimplemented!("Convert file_names to Vec<String>")
    }

    pub fn default_files() -> Self {
        Self::new(&["CLAUDE.md", ".mini-claw/instructions.md"])
    }

    pub fn discover(&self, start_dir: &Path) -> Vec<PathBuf> {
        unimplemented!(
            "Walk up from start_dir, collect matching files, reverse for root-first order"
        )
    }

    pub fn load(&self, start_dir: &Path) -> Option<String> {
        unimplemented!("Discover files, read each, join with headers showing source path")
    }

    pub fn system_prompt_section(&self, start_dir: &Path) -> Option<String> {
        unimplemented!("Call load(), wrap with instruction preamble")
    }
}
```

The loader is parameterized by file names to search for. The default
configuration looks for `CLAUDE.md` and `.mini-claw/instructions.md`.

### Rust concept: borrowed slices to owned collections

The constructor takes `&[&str]` -- a borrowed slice of borrowed string slices -- and converts it to `Vec<String>`. This is a common Rust pattern at API boundaries: accept borrowed data for flexibility (the caller can pass string literals, `&String`, or anything that derefs to `&str`), but store owned data internally so the struct has no lifetime parameter and can live independently of its creator.

### Implementing `new()`

The constructor converts the `&[&str]` slice into owned `String` values:

```rust
pub fn new(file_names: &[&str]) -> Self {
    Self {
        file_names: file_names.iter().map(|s| s.to_string()).collect(),
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

### `system_prompt_section()` -- wrapping for the prompt

The `system_prompt_section()` method calls `load()` and wraps the result with
an instruction preamble. This produces a string ready to insert into a system
prompt. If no instruction files are found, it returns `None`.

## Using InstructionLoader in a system prompt

In a production agent, the instruction loader is wired into the prompt assembly
pipeline. The loaded instructions are always dynamic -- they depend on which
directory the agent is launched from.

Here is how you might use `InstructionLoader` to build a simple system prompt:

```rust
let mut prompt = String::from("You are a coding agent.\n\n");

let loader = InstructionLoader::default_files();
if let Some(section) = loader.system_prompt_section(Path::new(cwd)) {
    prompt.push_str(&section);
}
```

A more sophisticated agent would separate static and dynamic sections for prompt
caching (see the concepts discussion above), but this simple approach works well
for getting started.

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

As an extension, you could build a `SystemPromptBuilder` that maintains separate
lists of static and dynamic sections, renders each half independently, and lets
a cache-aware provider split the prompt at the boundary. Our starter focuses on
the instruction loading piece, which is the most practically useful component.

## Running the tests

Run the InstructionLoader tests:

```bash
cargo test -p mini-claw-code-starter test_ch17
```

Note: The InstructionLoader tests are in `test_ch17`, not `test_ch5`. The test
file numbering follows the V1 chapter structure, not V2.

### What the tests verify

- **`test_ch17_instruction_loader_discover`**: Creates a temp directory with a
  CLAUDE.md file and verifies `discover()` finds it.
- **`test_ch17_instruction_loader_load`**: Same setup, verifies `load()` returns
  the file's content.
- **`test_ch17_instruction_loader_no_files`**: No instruction files exist.
  `load()` returns `None`.

## Recap

You have built the instruction loading infrastructure:

- **`InstructionLoader`** discovers CLAUDE.md files by walking up the filesystem.
  It concatenates them in root-first order so that global instructions appear
  before project-specific ones.
- **`system_prompt_section()`** wraps discovered instructions for inclusion in a
  system prompt.

You also learned the key concepts behind production system prompt architecture:

- **Prompt sections** break the system prompt into named, modular chunks.
- **The cache boundary** separates what changes from what does not, enabling
  prompt caching -- a single optimization that can cut costs and latency by an
  order of magnitude on long prompts. Every production agent does this.

As an extension, you could implement `PromptSection` and `SystemPromptBuilder`
types to manage the static/dynamic split structurally. The reference
implementation (`mini-claw-code`) shows one approach.

## Key takeaway

A system prompt is not a single string -- it is an assembly of modular sections, ordered so that stable content comes first (enabling prompt caching) and session-specific content comes last. The `InstructionLoader` is the simplest but most user-facing piece of this assembly: it gives every project a way to customize the agent's behavior through plain Markdown files.

## What's next

In [Chapter 6: File Tools](./ch06-file-tools.md) you will implement the tools
that let your agent interact with the filesystem -- reading, writing, and
editing files. These are the tools whose schemas will eventually appear in the
static portion of your system prompt.
