use serde::{Deserialize, Serialize};

// Json RPC request response
#[derive(Deserialize, Serialize, Debug)]
pub struct JsonRPCRequest {
    pub jsonrpc: String,
    pub id: u32,
    #[serde(flatten)]
    pub call: JsonRPCRequestMethodCall,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "method", content = "params")]
pub enum JsonRPCRequestMethodCall {
    #[serde(rename = "initialize")]
    Initialize(InitializeRequest),
    #[serde(rename = "session/new")]
    NewSession(NewSessionRequest),
    #[serde(rename = "session/prompt")]
    Prompt(PromptRequest),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct JsonRPCResponse<T> {
    pub jsonrpc: String,
    pub id: u32,
    pub result: T,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub call: JsonRPCNotificationMethod,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "method", content = "params")]
pub enum JsonRPCNotificationMethod {
    #[serde(rename = "session/update")]
    SessionUpdate(SessionUpdateResponse),
}

//Initialize

// Request
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub protocol_version: u32,
    pub client_capabilities: ClientCapabilities,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    pub fs: FileSystemCapabilities,
    pub terminal: bool,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemCapabilities {
    pub read_text_file: bool,
    pub write_text_file: bool,
}

// Response
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub protocol_version: u32,
    pub agent_capabilities: AgentCapabilities,
    pub auth_methods: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub load_session: bool,
    pub prompt_capabilities: PromptCapabilities,
    pub mcp: MCP,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapabilities {
    pub image: bool,
    pub video: bool,
    pub embeded_context: bool,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MCP {
    pub http: bool,
    pub sse: bool,
}

// New session

// Request
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionRequest {
    pub cwd: String,
    pub mcp_servers: Vec<MCPServer>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MCPServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<EnvironmentVariable>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

// Response
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionResponse {
    pub session_id: String,
}

// User message
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    pub session_id: String,
    pub prompt: Vec<AmpContentBlock>,
}

// Session Update
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateResponse {
    pub session_id: String,
    pub update: SessionUpdate,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "sessionUpdate")]
pub enum SessionUpdate {
    AgentMessageChunk(AgentMessageChunk),
    ToolCall(ToolCall),
    ToolCallUpdate(ToolCallUpdate),
    AgentThoughtChunk(AgentThoughtChunk),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct AgentMessageChunk {
    pub content: AmpContentBlock,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentThoughtChunk {
    pub content: AmpContentBlock,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub tool_call_id: String,
    pub title: String,
    pub kind: ToolKind,
    pub status: ToolCallStatus,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    Other,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdate {
    pub tool_call_id: String,
    pub status: ToolCallStatus,
    pub content: Vec<AgentToolCallResultContent>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum AgentToolCallResultContent {
    Content(AgentToolCallResultContentBlock),
    Diff(AgentToolCallResultDiffBlock),
    Follow(AgentToolCallResultFollowBlock),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResultContentBlock {
    pub content: AmpContentBlock,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResultDiffBlock {
    pub new_text: String,
    pub old_text: String,
    pub path: String,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResultFollowBlock {
    pub path: String,
    pub line: usize,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EndTurnResponse {
    pub stop_reason: String,
}

//messages

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum AmpContentBlock {
    Text(AmpTextContentBlock),
    Thinking(AmpThinkingContentBlock),
    ToolUse(AmpToolUseContentBlock),
    ToolResult(AmpToolResultContentBlock),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpTextContentBlock {
    pub text: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpThinkingContentBlock {
    pub thinking: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpToolUseContentBlock {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpToolResultContentBlock {
    #[serde(rename = "toolUseID")]
    pub tool_use_id: String,
    pub run: serde_json::Value,
}
