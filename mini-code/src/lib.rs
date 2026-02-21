pub mod agent;
pub mod mock;
pub mod providers;
pub mod streaming;
pub mod tools;
pub mod types;

#[cfg(test)]
mod tests;

pub use agent::{AgentEvent, SimpleAgent, single_turn};
pub use mock::MockProvider;
pub use providers::OpenRouterProvider;
pub use streaming::{
    MockStreamProvider, StreamAccumulator, StreamEvent, StreamProvider, StreamingAgent,
    parse_sse_line,
};
pub use tools::{BashTool, EditTool, ReadTool, WriteTool};
pub use types::*;
