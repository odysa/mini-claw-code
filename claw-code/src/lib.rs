pub mod engine;
pub mod prompt;
pub mod provider;
pub mod tools;
pub mod types;

// Phase 2+
pub mod agents;
pub mod config;
pub mod context;
pub mod hooks;
pub mod mcp;
pub mod permission;
pub mod session;
pub mod tui;

#[cfg(test)]
mod tests;

// Re-exports for convenience
pub use engine::QueryEngine;
pub use provider::{MockProvider, MockStreamProvider};
pub use tools::{BashTool, EditTool, GlobTool, GrepTool, ReadTool, ToolSet, WriteTool};
pub use types::*;
