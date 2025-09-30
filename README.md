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
- Authentication
- Agent plan

## Unsupported features
- Session load
- MCP (MCP servers configured on the client do not get passed through to AMP)
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
