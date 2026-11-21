---
egui-pop-agent: minor
---

The MCP server (`mcp` feature) gains the `widget_*` tools, which drive
the main window by pointer and pixels: `widget_tree`, `widget_get_widget`,
`widget_screenshot` (a PNG image item), `widget_wait_for`, `widget_click`,
`widget_hover`, `widget_scroll`, `widget_drag`, `widget_type_text`,
`widget_press_key`, `widget_resize` and `widget_batch` (several in one
call). They are Rerun's agent tools, `egui_inspection_tools` (by Lucas
Meurer and Emil Ernerfeldt, rerun-io/egui_crates, MIT OR Apache-2.0),
served over the socket's `inspect` through their `Transport`, so each
request is held like `inspect`: the bridge on, the agent's `ui` and
private grants, and the input lock. They are for testing by pointer and
pixels; `mcp::widget_tools` lists them. Until Rerun publishes
`egui_inspection_tools`, egui-pop-agent depends on its temporary,
unchanged fork, `egui-pop-inspection-tools`.
