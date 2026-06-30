"""TDG-Rust memory plugin — MemoryProvider for the Teleological Developmental Graph.

Native adapter that communicates with the TDG-Rust MCP server instead of
using the legacy Python TDG core. This provides:

- Per-turn memory recall via MCP tdg_search (hybrid FTS5 + embedding + graph)
- Turn persistence via MCP tdg_observe
- Mind state injection via MCP tdg_context
- Memory tool writes mirrored as observation nodes via MCP tdg_observe

Config: Requires tdg-rust MCP server configured in mcp_servers.tdg.
Override via env:
  TDG_RUST_BIN: path to tdg-rust binary (default: auto-detect from mcp config)
  TDG_MCP_TIMEOUT: MCP call timeout in seconds (default: 30)
"""

from __future__ import annotations

import json
import logging
import os
import subprocess
import threading
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

from agent.memory_provider import MemoryProvider
from tools.registry import tool_error

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# MCP Client — synchronous wrapper around tdg-rust serve
# ---------------------------------------------------------------------------

_TDG_RUST_BIN = os.environ.get(
    "TDG_RUST_BIN",
    str(Path.home() / ".hermes" / "tdg-rust" / "tdg-rust"),
)
_TDG_HOME = os.environ.get("TDG_HOME", str(Path.home() / ".hermes"))
_MCP_TIMEOUT = int(os.environ.get("TDG_MCP_TIMEOUT", "30"))

# LD_LIBRARY_PATH for ONNX runtime — required by tdg-rust binary
# Check Rust TDG lib first, fall back to Python TDG lib
_TDG_RUST_LIB = str(Path.home() / ".hermes" / "tdg-rust" / "lib")
_TDG_PYTHON_LIB = str(Path.home() / ".hermes" / "tdg" / "lib")
_TDG_LIB_DIR = _TDG_RUST_LIB if Path(_TDG_RUST_LIB).exists() else _TDG_PYTHON_LIB


class TdgMcpClient:
    """Synchronous MCP client for tdg-rust serve (stdio transport)."""

    def __init__(self, bin_path: str = _TDG_RUST_BIN, tdg_home: str = _TDG_HOME):
        self.bin_path = bin_path
        self.tdg_home = tdg_home
        self._request_id = 0

    def _next_id(self) -> int:
        self._request_id += 1
        return self._request_id

    def call_tool(self, tool_name: str, arguments: Dict[str, Any]) -> Dict[str, Any]:
        """Call an MCP tool and return the result."""
        import json as _json

        init_msg = _json.dumps({
            "jsonrpc": "2.0",
            "id": self._next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "tdg-adapter", "version": "1.0"},
            },
        })
        call_msg = _json.dumps({
            "jsonrpc": "2.0",
            "id": self._next_id(),
            "method": "tools/call",
            "params": {"name": tool_name, "arguments": arguments},
        })

        stdin_data = f"{init_msg}\n{call_msg}\n"

        try:
            result = subprocess.run(
                [self.bin_path, "serve"],
                input=stdin_data,
                capture_output=True,
                text=True,
                timeout=_MCP_TIMEOUT,
                env={
                    **os.environ,
                    "TDG_HOME": self.tdg_home,
                    "NO_COLOR": "1",
                    "LD_LIBRARY_PATH": _TDG_LIB_DIR,
                },
            )

            # Parse the last JSON line (tools/call response)
            for line in reversed(result.stdout.strip().split("\n")):
                line = line.strip()
                if line.startswith("{"):
                    try:
                        resp = _json.loads(line)
                        if "result" in resp:
                            content = resp["result"].get("content", [])
                            if content and content[0].get("type") == "text":
                                text = content[0]["text"]
                                try:
                                    return json.loads(text)
                                except (json.JSONDecodeError, ValueError):
                                    # Response is plain text (e.g. tdg_context returns markdown)
                                    return {"text": text}
                    except _json.JSONDecodeError:
                        continue

            return {"error": "No valid MCP response", "stderr": result.stderr[-500:]}

        except subprocess.TimeoutExpired:
            return {"error": f"MCP call timed out after {_MCP_TIMEOUT}s"}
        except FileNotFoundError:
            return {"error": f"tdg-rust binary not found at {self.bin_path}"}
        except Exception as e:
            return {"error": str(e)}


# ---------------------------------------------------------------------------
# Tool schemas — exposed to the LLM
# ---------------------------------------------------------------------------

TDG_MEMORY_SEARCH_SCHEMA = {
    "name": "tdg_memory_search",
    "description": (
        "Search TDG memory graph for relevant context. "
        "Uses hybrid FTS5 + embedding + graph expansion for best results.\n\n"
        "Use before answering questions about the user, their projects, "
        "or past conversations."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query text",
            },
            "limit": {
                "type": "integer",
                "description": "Max results (default: 10)",
            },
            "node_type": {
                "type": "string",
                "description": "Filter by node type (observation, skill, action, etc.)",
            },
        },
        "required": ["query"],
    },
}

TDG_MEMORY_RECORD_SCHEMA = {
    "name": "tdg_memory_record",
    "description": (
        "Record an observation in the TDG memory graph. "
        "Use to persist important information, decisions, or events."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "description": {
                "type": "string",
                "description": "What was observed",
            },
            "entities": {
                "type": "string",
                "description": "Comma-separated entity names",
            },
            "quadrant": {
                "type": "string",
                "description": "Quadrant: LR, UL, LL, UR",
            },
        },
        "required": ["description"],
    },
}

TDG_MEMORY_STATUS_SCHEMA = {
    "name": "tdg_memory_status",
    "description": "Get TDG memory graph status and health.",
    "parameters": {"type": "object", "properties": {}},
}


# ---------------------------------------------------------------------------
# TDGMemoryProvider — MemoryProvider implementation
# ---------------------------------------------------------------------------

class TDGMemoryProvider(MemoryProvider):
    """TDG-Rust native memory provider — communicates via MCP server."""

    def __init__(self):
        self._client: Optional[TdgMcpClient] = None
        self._available: bool = False
        self._initialized: bool = False

    @property
    def name(self) -> str:
        return "tdg"

    def is_available(self) -> bool:
        """Check if tdg-rust binary exists and is executable."""
        if self._available:
            return True

        bin_path = Path(_TDG_RUST_BIN)
        if not bin_path.exists():
            logger.debug("tdg-rust binary not found at %s", bin_path)
            return False

        if not os.access(bin_path, os.X_OK):
            logger.debug("tdg-rust binary not executable at %s", bin_path)
            return False

        # Quick health check
        client = TdgMcpClient(str(bin_path), _TDG_HOME)
        result = client.call_tool("tdg_system_health", {})
        if "error" in result:
            logger.debug("tdg-rust health check failed: %s", result["error"])
            return False

        self._available = True
        return True

    def initialize(self, session_id: str, **kwargs) -> None:
        """Initialize the MCP client connection."""
        if self._initialized:
            return

        hermes_home = kwargs.get("hermes_home", _TDG_HOME)
        self._client = TdgMcpClient(_TDG_RUST_BIN, hermes_home)
        self._initialized = True
        logger.info("TDG-Rust memory provider initialized (session=%s)", session_id)

    def system_prompt_block(self) -> str:
        """Return terrain context for system prompt."""
        if not self._client:
            return ""

        # tdg_context returns markdown text, not JSON — use raw call
        try:
            result = self._client.call_tool("tdg_context", {})
        except Exception as e:
            logger.warning("Failed to get TDG context: %s", e)
            return ""

        if isinstance(result, dict):
            if "error" in result:
                logger.warning("TDG context error: %s", result["error"])
                return ""
            # If the result has a terrain key, use it
            if "terrain" in result:
                return result.get("terrain", "")
            # If it has content that's already a string, use it
            if "text" in result:
                return result["text"]
        elif isinstance(result, str):
            return result
        return ""

    def prefetch(self, query: str, *, session_id: str = "") -> str:
        """Recall relevant context for the upcoming turn."""
        if not self._client:
            return ""

        result = self._client.call_tool("tdg_search", {
            "query": query,
            "limit": 5,
        })

        if "error" in result:
            logger.warning("TDG prefetch failed: %s", result["error"])
            return ""

        # Format results as context
        nodes = result.get("nodes", [])
        if not nodes:
            return ""

        lines = []
        for node in nodes[:5]:
            name = node.get("name", "")[:100]
            node_type = node.get("node_type", "unknown")
            score = node.get("score", 0)
            lines.append(f"[{node_type}] {name} (relevance: {score:.2f})")

        return "TDG Memory Recall:\n" + "\n".join(lines)

    def sync_turn(self, user_message: str, assistant_response: str, **kwargs) -> None:
        """Persist conversation turn as observation node.

        Only records substantive turns — skips short/echo messages.
        Extracts the key action or decision, not the raw transcript.
        """
        if not self._client:
            return

        # Skip trivial turns (greetings, acknowledgments, echoes)
        user_len = len(user_message.strip())
        asst_len = len(assistant_response.strip())
        if user_len < 20 or asst_len < 30:
            return

        # Skip if the assistant response is mostly tool output or code
        tool_heavy = assistant_response.count("```") >= 2 or assistant_response.count("function") > 3
        if tool_heavy and user_len < 100:
            return

        # Extract a concise observation (not the full transcript)
        user_summary = user_message[:200].replace("\n", " ").strip()
        asst_summary = assistant_response[:300].replace("\n", " ").strip()
        description = f"User asked: {user_summary}\nAgent did: {asst_summary}"

        result = self._client.call_tool("tdg_observe", {
            "text": description,
            "description": description,
            "trigger_digestion": False,
        })

        if "error" in result:
            logger.warning("TDG sync_turn failed: %s", result["error"])

    def get_tool_schemas(self) -> List[Dict[str, Any]]:
        """Return tool schemas to expose to the model."""
        return [
            TDG_MEMORY_SEARCH_SCHEMA,
            TDG_MEMORY_RECORD_SCHEMA,
            TDG_MEMORY_STATUS_SCHEMA,
        ]

    def handle_tool_call(self, tool_name: str, arguments: Dict[str, Any]) -> str:
        """Dispatch a tool call to the TDG-Rust MCP server."""
        if not self._client:
            return json.dumps({"error": "TDG-Rust provider not initialized"})

        # Map tool names to MCP tool names
        tool_map = {
            "tdg_memory_search": "tdg_search",
            "tdg_memory_record": "tdg_observe",
            "tdg_memory_status": "tdg_graph_stats",
        }

        mcp_tool = tool_map.get(tool_name, tool_name)
        result = self._client.call_tool(mcp_tool, arguments)
        return json.dumps(result)

    def shutdown(self) -> None:
        """Clean up resources."""
        self._client = None
        self._initialized = False
        self._available = False
        logger.info("TDG-Rust memory provider shut down")

    def on_memory_write(self, action: str, target: str, content: str,
                        metadata: Optional[Dict[str, Any]] = None) -> None:
        """Mirror built-in memory writes as TDG observation nodes.

        When the agent uses the built-in `memory` tool, this hook fires
        so the same fact gets persisted in the TDG graph for recall.
        """
        if not self._client:
            return

        # Build a concise observation from the memory write
        desc = f"Memory {action} ({target}): {content[:500]}"
        try:
            self._client.call_tool("tdg_observe", {
                "text": desc,
                "description": desc,
                "trigger_digestion": False,
            })
        except Exception:
            pass  # Best-effort — never block the agent

    def on_session_end(self, messages: list) -> None:
        """End-of-session extraction — create a summary observation."""
        if not self._client or not messages:
            return

        # Count turns and create a session summary
        turn_count = sum(1 for m in messages if m.get("role") == "user")
        if turn_count < 3:
            return  # Skip short sessions

        desc = f"Session ended: {turn_count} turns. "
        # Extract last user message as session theme
        for msg in reversed(messages):
            if msg.get("role") == "user":
                content = msg.get("content", "")[:200]
                if content:
                    desc += f"Last topic: {content.replace(chr(10), ' ')}"
                break

        try:
            self._client.call_tool("tdg_observe", {
                "text": desc,
                "description": desc,
                "trigger_digestion": False,
            })
        except Exception:
            pass


# ---------------------------------------------------------------------------
# Plugin registration
# ---------------------------------------------------------------------------

_provider_instance: Optional[TDGMemoryProvider] = None


def register(ctx: Any = None) -> None:
    """Register the TDG-Rust memory provider with the plugin system."""
    global _provider_instance
    if _provider_instance is None:
        _provider_instance = TDGMemoryProvider()
    if ctx is not None and hasattr(ctx, "register_memory_provider"):
        ctx.register_memory_provider(_provider_instance)


def register_memory_provider(ctx: Any = None) -> TDGMemoryProvider:
    """Legacy registration function."""
    register(ctx)
    return _provider_instance
