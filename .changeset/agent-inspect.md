---
egui-pop-agent: minor
---

The agent socket gains `inspect`: egui's inspection protocol
(`egui_inspection`, by Lucas Meurer and Emil Ernerfeldt) carried over
the socket to an in-process `InspectionPlugin`, with no TCP port. An
agent gets the full accessibility tree, a screenshot of the main window,
any egui event (drags, scroll deltas, clicks at a position), a resize
and a wait for the frame to settle. It needs the bridge on and the
agent's `ui` and private grants, and is refused while the user answers
a confirmation or a pairing. Tunnelling the protocol is the idea of
Rerun's `egui_mcp` and `egui_inspection_tools` (rerun-io/egui_crates).
`egui_pop_agent::inspect::request_to_json` and `response_from_json`
convert its requests and responses.
