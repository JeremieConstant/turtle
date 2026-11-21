# Every capability is one operation, exposed identically through UI, API and MCP

Agents must be able to do everything a person can. Every capability is therefore defined once as an operation in the core, and the web UI, the HTTP API and the MCP server are thin adapters over that layer; nothing may exist only in the UI. A CI test fails when an operation has no MCP tool. Permission checks, including the filtering of search results, live in this layer so that all three entry points behave the same.

The MCP server is built into the binary (Streamable HTTP, token authentication) and is not a separate process.
