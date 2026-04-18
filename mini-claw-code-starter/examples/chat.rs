// Bonus: Interactive CLI (no book chapter yet — self-guided)
//
// Wire the agent you built in Chapters 1-10 into an interactive REPL.
// The reference implementation is at `mini-claw-code/examples/chat.rs`;
// fill in this stub to build your own without peeking. Run it with:
//     cargo run -p mini-claw-code-starter --example chat
//
// Steps:
// 1. Import types: BashTool, EditTool, Message, OpenRouterProvider, ReadTool, SimpleAgent, WriteTool
// 2. Create an OpenRouterProvider using from_env()
// 3. Build a SimpleAgent with all four tools (Bash, Read, Write, Edit)
// 4. Create a Vec<Message> to hold the conversation history
// 5. Loop: print "> ", read a line from stdin, push Message::User, call agent.chat(), print result
// 6. Break on EOF (Ctrl+D)

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    unimplemented!(
        "Create provider, build agent with tools, loop reading stdin, push to history, call chat(), print result"
    )
}
