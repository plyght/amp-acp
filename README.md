# AMP ACP Wrapper

This is an inprogress implementation of AMP ACP Wrapper.

## Supported platforms
The project has only been tested this on Macos 26 so far

## Supported features
- Create Session
- Streamed messages
- Thinking
- File edits rendered as diff in chat
- Cancellation

## Unsupported features
- Resources
- Authentication
- Session load
- MCP (MCP servers configured via AMP do work but MCP servers configured on the client do not)
- Agent plan
- Follow Agent
- Non text content types

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
