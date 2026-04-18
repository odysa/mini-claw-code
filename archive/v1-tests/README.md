# V1 test numbering (historical)

This directory exists only as a historical marker. The starter and reference
crates no longer use V1 chapter numbering for their tests — everything has
been renamed by topic (see `mini-claw-code-starter/src/tests/` and
`mini-claw-code/src/tests/`).

During the V1 → V2 transition the test files were named `ch{N}.rs` matching
the V1 chapter that originally introduced them. That left callers running
`cargo test test_ch19` for the V2 permissions chapter — a jarring mismatch.
The final reorg split each file by topic and renamed every `test_chN_` prefix
to a topic name:

| V1 file   | Topic file                         | Function prefix            |
|-----------|------------------------------------|----------------------------|
| ch1.rs    | `mock.rs`                          | `test_mock_*`              |
| ch2.rs    | `read.rs`                          | `test_read_*`              |
| ch3.rs    | `single_turn.rs`                   | `test_single_turn_*`       |
| ch4.rs    | `bash.rs` + `write.rs` + `edit.rs` | `test_bash_*`, etc.        |
| ch5.rs    | `simple_agent.rs`                  | `test_simple_agent_*`      |
| ch6.rs    | `openrouter.rs`                    | `test_openrouter_*`        |
| ch7.rs    | `multi_tool.rs`                    | `test_multi_tool_*`        |
| ch10.rs   | `streaming.rs`                     | `test_streaming_*`         |
| ch11.rs   | `ask.rs`                           | `test_ask_*` (bonus)       |
| ch12.rs   | `plan_agent.rs`                    | `test_plan_*`              |
| ch13.rs   | `subagent.rs`                      | `test_subagent_*` (bonus)  |
| ch14.rs   | `cost_tracker.rs`                  | `test_cost_tracker_*`      |
| ch15.rs   | `context_manager.rs`               | `test_context_manager_*`   |
| ch16.rs   | `config.rs`                        | `test_config_*`            |
| ch17.rs   | `instructions.rs`                  | `test_instructions_*`      |
| ch18.rs   | `safety.rs`                        | `test_safety_*`            |
| ch19.rs   | `permissions.rs`                   | `test_permissions_*`       |
| ch20.rs   | `hooks.rs`                         | `test_hooks_*`             |
| ch21.rs   | `mcp.rs` (reference crate only)    | `test_mcp_*`               |

V2 book chapters carry the matching topic in each "Test to run" callout, so
there is no more chapter/test number discord. This folder is kept just as a
pointer for anyone who encounters V1-era references in old issues, PRs, or
discussions.
