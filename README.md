# AMP ACP Wrapper

This is an inprogress implementation of AMP ACP Wrapper.

## Supported platforms
The project has only been tested this on Macos 26 so far

## Supported features
- Create Session
- Thinking
- File edits rendered as diff in chat
- Cancellation
- Resources
- Authentication (API key)
- Agent plan
- MCP server pass-through (stdio, HTTP, SSE)
- Image input support
- Audio input support
- Embedded context/resources
- Streaming via --stream-json (real-time, no polling)

## Unsupported features
- Session load (partially implemented)
- Follow Agent
- Session modes

## Installation
Currently the project needs to be built from source.
### Pre-requisites
- [Rust](https://rustup.rs/)

### How to build
```bash
git clone https://github.com/Hamish-taylor/amp-acp.git
cd amp-acp
cargo build --release
```

### Usage
Add the agent to your IDE/client.
In zed you can do this throught the settings.json file. It should look something like this:
```
"agent_servers": {
    "amp": {
      "command": "/path-to-cloned-repo/target/release/amp-acp",
      "args": [],
      "env": {}
    },
  },
```
