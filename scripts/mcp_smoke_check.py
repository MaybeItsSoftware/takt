#!/usr/bin/env python3
"""Check that `Takt --mcp-server` still reaches an MCP server.

This replaces `mcp_parity_check.py`. That harness existed because Priority
shipped two implementations of the same MCP server — one in Swift inside the
app, one in the Rust CLI — and a client could be pointed at either, so a tool
that answered differently in one was a capability the assistant appeared to lose
depending on how it was wired up. It drove both over stdio and diffed their tool
lists, their answers, the files they left on disk, and their HTTP requests.

There is now one implementation. The Swift server is gone; the app bundles the
CLI at `Contents/Helpers/takt` and `--mcp-server` hands the process over to
it (see `exec_mcp_server` in `desktop/src/platform`). Correctness of the
server itself is `cargo test`'s job.

What is left to check is the seam — that the handover works, and in particular
that a client configuration written *before* this change, naming
`Priority --mcp-server` with credentials in `env`, still gets a working server
once that path names `Takt.app/Contents/MacOS/Takt` instead.
That is the compatibility promise the migration was built on, and nothing else
covers it.

Reads no real data and needs no credentials. Needs the app bundle:
`TAKT_BUNDLE_ONLY=1 scripts/install_desktop.sh` (or an installed /Applications/Takt.app).
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
EXPECTED_TOOL_COUNT = 54
PROTOCOL_VERSION = "2024-11-05"


def find_app() -> pathlib.Path | None:
    """The bundle install_desktop.sh assembles, else the installed one."""
    for app in (REPO / "build/desktop/Takt.app", pathlib.Path("/Applications/Takt.app")):
        if (app / "Contents/MacOS/Takt").exists():
            return app
    return None


def speak_mcp(argv: list[str], env: dict[str, str]) -> list[dict]:
    """Drive one server over stdio and return its replies."""
    requests = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "smoke", "version": "1"},
            },
        },
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
    ]
    stdin = "".join(json.dumps(r) + "\n" for r in requests)
    result = subprocess.run(
        argv, input=stdin, env=env, capture_output=True, text=True, timeout=60
    )
    replies = []
    for line in result.stdout.splitlines():
        line = line.strip()
        if line:
            replies.append(json.loads(line))
    if not replies:
        raise SystemExit(
            f"no MCP replies from {' '.join(argv)}\n"
            f"exit={result.returncode}\nstderr:\n{result.stderr}"
        )
    return replies


def main() -> int:
    app = find_app()
    if app is None:
        print(
            "error: no Takt.app found. Bundle it first:\n"
            "  TAKT_BUNDLE_ONLY=1 scripts/install_desktop.sh",
            file=sys.stderr,
        )
        return 2

    helper = app / "Contents/Helpers/takt"
    if not helper.exists():
        print(
            f"error: {helper} is missing — the app has no MCP server.\n"
            "       scripts/install_desktop.sh copies it in while it bundles the app.",
            file=sys.stderr,
        )
        return 1

    # Deliberately not the real config location: an isolated store, and
    # credentials passed the way a client configuration passes them.
    env = {
        "HOME": str(REPO / "build" / "mcp-smoke-home"),
        "PATH": "/usr/bin:/bin",
        "CHECKVIST_USERNAME": "smoke@example.com",
        "CHECKVIST_REMOTE_KEY": "not-a-real-key",
    }
    pathlib.Path(env["HOME"]).mkdir(parents=True, exist_ok=True)

    invocations = {
        # How configurations written before the migration name it.
        "Takt --mcp-server": [str(app / "Contents/MacOS/Takt"), "--mcp-server"],
        # How they are written now.
        "takt mcp": [str(helper), "mcp"],
    }

    tool_lists = {}
    for label, argv in invocations.items():
        replies = speak_mcp(argv, env)
        by_id = {r.get("id"): r for r in replies}

        initialize = by_id.get(1, {}).get("result")
        if not initialize:
            print(f"error: {label} did not answer initialize: {by_id.get(1)}", file=sys.stderr)
            return 1
        if initialize.get("protocolVersion") != PROTOCOL_VERSION:
            print(
                f"error: {label} answered protocolVersion "
                f"{initialize.get('protocolVersion')!r}, expected {PROTOCOL_VERSION!r}",
                file=sys.stderr,
            )
            return 1

        tools = by_id.get(2, {}).get("result", {}).get("tools")
        if tools is None:
            print(f"error: {label} did not answer tools/list: {by_id.get(2)}", file=sys.stderr)
            return 1
        tool_lists[label] = sorted(t["name"] for t in tools)
        print(f"{label}: initialize ok, {len(tools)} tools")

    labels = list(tool_lists)
    if tool_lists[labels[0]] != tool_lists[labels[1]]:
        print(
            "error: the two invocations expose different tools\n"
            f"  {labels[0]}: {tool_lists[labels[0]]}\n"
            f"  {labels[1]}: {tool_lists[labels[1]]}",
            file=sys.stderr,
        )
        return 1

    count = len(tool_lists[labels[0]])
    if count != EXPECTED_TOOL_COUNT:
        print(
            f"error: expected {EXPECTED_TOOL_COUNT} tools, found {count}.\n"
            "       If a tool was added or removed on purpose, update "
            "EXPECTED_TOOL_COUNT and docs/mcp-server.md.",
            file=sys.stderr,
        )
        return 1

    print(
        "\nthe --mcp-server shim reaches the bundled CLI, and configurations "
        "written before the migration still work."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
