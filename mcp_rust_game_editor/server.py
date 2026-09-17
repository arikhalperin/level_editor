#!/usr/bin/env python3
"""MCP stdio server: exposes `get_editor_debug` to read editor_debug.json from the app cache."""
from __future__ import annotations

import asyncio
import json
import os
import sys
from pathlib import Path

from mcp.server.fastmcp import FastMCP
from platformdirs import user_cache_dir

mcp = FastMCP(
    "rust-game-editor-debug",
    instructions=(
        "Reads editor_debug.json written by rust_game_editor when minimap debug export is enabled "
        "(debug build or RUST_GAME_EDITOR_DEBUG=1)."
    ),
)


def editor_debug_path() -> Path:
    override = os.environ.get("RUST_GAME_EDITOR_DEBUG_PATH")
    if override:
        return Path(override).expanduser()
    cache = Path(user_cache_dir("rust_game_editor", appauthor=False))
    return cache / "editor_debug.json"


@mcp.tool()
def get_editor_debug() -> str:
    """Read editor_debug.json (minimap panel rects, scroll, texture flags)."""
    path = editor_debug_path()
    if not path.is_file():
        return (
            f"editor_debug.json not found at {path}. "
            "Run the editor with RUST_GAME_EDITOR_DEBUG=1 or a debug build, "
            "then keep a frame open so the minimap runs."
        )
    try:
        raw = path.read_text(encoding="utf-8")
        try:
            data = json.loads(raw)
            return json.dumps(data, indent=2)
        except json.JSONDecodeError:
            return raw
    except OSError as e:
        return f"Failed to read {path}: {e}"


def main() -> int:
    asyncio.run(mcp.run_stdio_async())
    return 0


if __name__ == "__main__":
    sys.exit(main())
