---
name: rpshop-automation
description: Inspect or edit a live Ingenaro prototype with Ingenaro tools. Use for project files, pages and folders, widgets, styles, interactions, editor history, preview, or export.
---

# Ingenaro live editing

In Ingenaro's **文件** menu, open **外部 API / MCP** and enable MCP for the current project. Use **复制 MCP 配置（含临时密钥）** to configure an external client. Keep the temporary bearer token in the client configuration. Built-in AI receives the tools directly. The optional `ingenaro --mcp-stdio` bridge accepts `INGENARO_MCP_TOKEN` and the legacy `RPSHOP_MCP_TOKEN` alias.

1. Discover the available tools with MCP `tools/list` or the host's supplied definitions. Call `rpshop_capabilities` to check format, write limits and scope. Use the live tools for document work and the host dialogs for opening, saving and exporting files.
2. Read the relevant live state with `rpshop_read_pages`, `rpshop_read_layers`, `rpshop_read_widgets`, `rpshop_read_styles`, or `rpshop_read_interactions`. Page and widget IDs are positive numbers. Folder IDs are strings. An empty `page_tree` means that all pages are at the root.
3. Use `rpshop_batch_edit` for atomic document edits. A failed operation rolls back the batch. Use explicit page IDs for multi-page work. Search `rpshop_read_library` before creating a preset with `create_library_item`. Read the live state again to verify the result, and report only confirmed edits.

## Organize the page directory

Check `page_folders.editable` in capabilities. In `.ingenaro`, create folders with `create_page_folder`. Provide a unique `folder_id` to reference a folder in the same batch, or use the returned ID in the next call. Set `parent_folder_id` for a subfolder. Use `move_page` for an existing page and `create_page` with `folder_id` for a new page inside a folder. Root destinations use null or omit the destination field. Use `rename_page_folder`, `move_page_folder`, and `delete_page_folder` to manage folders. Deletion promotes children to the parent level and preserves the pages. For native RP files, the folder tree is readable and can be edited after saving as `.ingenaro`.

This `rpshop_batch_edit` call groups page 1 with a new settings page. Confirm that page 1 exists before calling.

```json
{"operations":[
  {"op":"create_page_folder","folder_id":"account","name":"账户"},
  {"op":"move_page","page_id":1,"folder_id":"account"},
  {"op":"create_page","name":"账户设置","folder_id":"account"}
]}
```

Page-local workers edit the assigned page. The coordinating session handles directory changes.

## Use the live editor

- Use `rpshop_edit_widgets` for alignment, distribution, quarter-turn rotation, reflection and duplication. Pass explicit `widget_ids` and use the returned IDs for duplicates. Set `target_page_id` on `duplicate` to copy to another page.
- Use `rpshop_read_view` and `rpshop_set_view` for selection and the viewport. Use `rpshop_history` when the user requests undo or redo. The editor maintains the shared history.
- Use `rpshop_open_project` or `rpshop_new_project` when the user requests a project switch. The host handles unsaved changes. Start a new turn after a successful switch.
- Use `rpshop_preview`, `rpshop_save`, `rpshop_export`, and `rpshop_publish_html` according to the requested deliverable. Respect tool approvals and report any cancelled or pending dialog as such.

## Application plugins

Use `ingenaro_list_plugins` with `{}` to read the application plugin catalog and installation state. Call `ingenaro_open_plugin_manager` with `{}` to open **添加插件** after approval. The user chooses **AI 工具** in the catalog. The tool returns whether the catalog opened. Read the inventory again to check whether the user added the plugin.

To use Antigravity in the AI Tool, install `agy` 1.5.3 or later, sign in from the terminal, then select **Antigravity** in **AI 设置**. Antigravity receives the live project tool inventory through the authenticated per-turn MCP server. Use the same capability checks and editor operations as other engines. Page delegation requires an engine whose live inventory includes the delegation tools.

Inside the AI Tool, users can configure a trusted custom engine. It receives its per-turn inventory in the `start` frame. Use the supplied live tools and wait for each `tool_result`. The host applies project, permission, cancellation and undo rules. Page delegation is available when the inventory includes the delegation tools.

The host queues user submissions made with Send or Enter. A user can steer a running turn from the queued message's **引导对话** control. The host removes that entry when the engine acknowledges receipt.
