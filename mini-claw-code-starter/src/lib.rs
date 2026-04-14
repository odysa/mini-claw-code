#![allow(unused, dead_code)]

pub mod agent;
pub mod config;
pub mod context;
pub mod hooks;
pub mod instructions;
pub mod mock;
pub mod permissions;
pub mod planning;
pub mod providers;
pub mod safety;
pub mod streaming;
pub mod subagent;
pub mod tools;
pub mod types;
pub mod usage;

#[cfg(test)]
mod tests;

pub use agent::{AgentEvent, SimpleAgent, single_turn};
pub use config::{Config, ConfigLoader};
pub use context::ContextManager;
pub use hooks::{Hook, HookAction, HookEvent, HookRegistry};
pub use instructions::InstructionLoader;
pub use mock::MockProvider;
pub use permissions::{Permission, PermissionEngine, PermissionRule};
pub use planning::PlanAgent;
pub use providers::OpenRouterProvider;
pub use safety::{CommandFilter, PathValidator, SafeToolWrapper, SafetyCheck};
pub use streaming::{
    MockStreamProvider, StreamAccumulator, StreamEvent, StreamProvider, StreamingAgent,
    parse_sse_line,
};
pub use subagent::SubagentTool;
pub use tools::{
    AskTool, BashTool, ChannelInputHandler, CliInputHandler, EditTool, InputHandler,
    MockInputHandler, ReadTool, UserInputRequest, WriteTool,
};
pub use types::*;
pub use usage::CostTracker;
