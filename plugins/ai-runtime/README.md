# AI Design Runtime

`ai-design-runtime` is the bundled AI engine package used by Ingenaro. A Rust host can depend on this package and supply its own tool inventory, project scope, prompts and optional native page delegation. The host supplies the document model, desktop UI, credential vault and project format.

## Integrate with another host

1. Implement `ToolClient`. Return MCP-style definitions with `name`, `description` and `inputSchema`. Authorize every call in the host and check the turn scope, turn cancellation and per-call interruption immediately before committing. The host owns document validation, transaction rollback, undo and file dialogs.
2. Supply a `HostProfile` with product identity, tool revision and design instructions. Set `native_design` if the host implements scoped page delegation through an authenticated loopback MCP server. Set it to `None` for a single-agent host.
3. Start the turn with `AiRuntime`. Consume `RunningSession.events` and use `SessionControl` to cancel or answer engine permission requests. Bind the tool client to the same cancellation flag supplied in `StartRequest`. Store credentials, chat history and project bindings in the host.

The Ingenaro adapter is in [src/ai.rs](../../src/ai.rs). Run `cargo test -p ai-design-runtime --locked` from the workspace to test the package independently.

## Bundled Antigravity CLI engine

Install `agy` 1.5.3 or later and sign in by running `agy` in a terminal. The workbench discovers it alongside Claude Code and Codex. It uses the CLI's ACP interface for model discovery, image prompts, streaming and conversation resume. Design turns use the host's authenticated loopback MCP endpoint and the existing project permissions and undo. Page delegation is available through Claude Code and Codex.

## Executable engine protocol v1

In the Ingenaro toolbar, open **添加插件** and add **AI 工具**. Inside that tool, open **AI 设置 → 添加自定义引擎…** and select a trusted local executable. Windows accepts `.exe`. All platforms also accept `.mjs` with Node.js on PATH. The host stores the selection in its own settings and launches the plugin when the user sends a turn. Removing the configuration leaves the file on disk.

The plugin runs with the current user's OS permissions. The application supplies the current turn's prompt, selected context, attached images and permitted tool definitions. MCP bearer tokens and vault keys stay in the host. The plugin manages its own model and authentication. Document tool calls still pass through the editor's permission, scope and format checks.

Messages are UTF-8 JSON Lines over stdin and stdout. Flush each outgoing line. The host starts one process per turn, uses stdout for protocol messages and terminates the process when the turn ends or is cancelled.

| Direction | Frame | Fields |
| --- | --- | --- |
| Host → plugin | `hello` | `protocol: "ai-design-engine"`, `version: 1` |
| Plugin → host | `hello` | Same protocol and version within 5 seconds. The host checks this before sending any design data. |
| Host → plugin | `start` | `prompt` as a string, `images` as an array of `{name, data_url}`, and `tools` as an array of MCP-style definitions |
| Plugin → host | `text` | `text` as a string. Multiple frames append to the assistant response. |
| Plugin → host | `tool_call` | Unique per-turn string `id` of at most 128 bytes, tool `name`, and object `arguments` |
| Host → plugin | `tool_result` | The same `id`, JSON `result`, and boolean `is_error`. Wait for this result before issuing the next call. |
| Plugin → host | `done` | Completes the turn. |
| Plugin → host | `error` | Ends the turn and displays a bounded `message`. |

Each frame has a string `type` with the value in the Frame column. The plugin can issue up to 128 calls per turn. Host frames are limited to 20 MiB and tool results to 10 MiB. Plugin messages are limited to 1 MiB after parsing. The desktop turn timeout is five minutes, including approval waits. Protocol v1 supports text, image inputs and live tool calls. Provider-side resume, in-flight steering and native subagent delegation use the bundled CLI integrations.

The runnable [example plugin](../../examples/ai-plugin.mjs) requires no model or network. Select it in settings, then send **添加示例标题** in native canvas mode to verify a tool call and undo.

For automation clients, `rpshop_capabilities` reports the application plugin tools. Call `ingenaro_list_plugins` to inspect the catalog and installation state. `ingenaro_open_plugin_manager` requests approval to open the catalog. The user chooses the application plugin in the host and can then configure its engine.
