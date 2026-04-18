# 第九章：更好的 TUI

`chat.rs` 命令行界面虽然能用，但它只会输出纯文本，并且显示每一次工具调用。一个真正的 coding agent 应该具备 Markdown 渲染、思考动画以及在 agent 忙碌时折叠工具调用的能力。

参见 `mini-claw-code/examples/tui.rs` 中的参考实现。它使用了：

- **`termimad`**：在终端中进行内联 Markdown 渲染。
- **`crossterm`**：用于原始终端模式（raw terminal mode），在第十一章的方向键选择 UI 中会用到。
- **加载动画**（animated spinner）(`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`)：在 agent 思考时循环播放。
- **折叠工具调用**：超过 3 次工具调用后，后续调用会折叠为 `... and N more` 计数器，保持输出整洁。

TUI 构建在第十章 `StreamingAgent` 的 `AgentEvent` 流之上。事件循环使用 `tokio::select!` 来多路复用三个事件源：

1. **agent 事件**（`AgentEvent::TextDelta`、`ToolCall`、`Done`、`Error`）——渲染流式文本、工具调用摘要或最终输出。
2. **用户输入请求**，来自 `AskTool`（第十一章）——暂停加载动画，显示文本提示或方向键选择列表。
3. **定时器心跳**（Timer ticks）——推进加载动画。

本章仅为讲解说明，无需编写代码。请阅读 `examples/tui.rs` 了解各部分如何协同工作，或者让你的 mini-claw-code agent 为你构建一个 TUI。
