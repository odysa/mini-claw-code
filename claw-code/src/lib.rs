pub mod engine;
pub mod prompt;
pub mod provider;
pub mod tools;
pub mod types;

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
pub use agents::PlanEngine;
pub use config::{Config, ConfigLoader, CostTracker};
pub use engine::QueryEngine;
pub use hooks::{Hook, HookAction, HookEvent, HookRunner};
pub use permission::{PermissionEngine, SafetyChecker};
pub use provider::{MockProvider, MockStreamProvider};
pub use tools::{BashTool, EditTool, GlobTool, GrepTool, ReadTool, ToolSet, WriteTool};
pub use types::*;
