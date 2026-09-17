# rust_game_editor MCP debug bridge

This folder contains a small **stdio MCP server** that exposes one tool, `get_editor_debug`, which reads the JSON file the editor writes when minimap debug export is enabled.

## What gets written

The editor writes **`editor_debug.json`** to the OS cache directory for the app id `rust_game_editor` (same layout as the Rust `dirs` crate):

| OS | Typical path |
|----|----------------|
| macOS | `~/Library/Caches/rust_game_editor/editor_debug.json` |
| Linux | `~/.cache/rust_game_editor/editor_debug.json` |
| Windows | `%LOCALAPPDATA%\rust_game_editor\editor_debug.json` |

**When it is written**

- `cargo run` / **debug builds**: every frame while the minimap runs (can be disabled by building release without the env var).
- **Release** builds: only if `RUST_GAME_EDITOR_DEBUG=1` is set in the environment.

**Read from a different file (MCP only)**

Set `RUST_GAME_EDITOR_DEBUG_PATH` to an absolute path when starting the MCP server; the tool reads that path instead of the cache file. Use this if you copy `editor_debug.json` somewhere else.

## Setup

```bash
cd mcp_rust_game_editor
python3 -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

## Cursor MCP configuration

Add a server (e.g. in **Cursor Settings → MCP**) using **stdio** with the venv interpreter so `mcp` and `platformdirs` resolve:

**Example (macOS/Linux)**

- **Command**: `/absolute/path/to/rust_game_editor/mcp_rust_game_editor/.venv/bin/python`
- **Args**: `/absolute/path/to/rust_game_editor/mcp_rust_game_editor/server.py`

**Example (Windows)**

- **Command**: `C:\path\to\rust_game_editor\mcp_rust_game_editor\.venv\Scripts\python.exe`
- **Args**: `C:\path\to\rust_game_editor\mcp_rust_game_editor\server.py`

After the editor has run at least one frame with debug export enabled, invoke the tool **`get_editor_debug`** in chat; you should see pretty-printed JSON with `panel_clip_*`, `map_rect_*`, `scroll_offset`, `has_minimap_texture`, etc.

## Tool

| Name | Description |
|------|-------------|
| `get_editor_debug` | Returns the contents of `editor_debug.json`, or a short message if the file is missing. |

## Requirements

See `requirements.txt` (`mcp`, `anyio`, `platformdirs`).
