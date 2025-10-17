use agent_client_protocol::{
    Agent, AgentCapabilities, AgentSideConnection, AuthMethod, AuthMethodId, AuthenticateRequest,
    AuthenticateResponse, CancelNotification, Client, ContentBlock, Diff, EmbeddedResourceResource,
    Error, ExtNotification, ExtRequest, ExtResponse, InitializeRequest, InitializeResponse,
    LoadSessionRequest, LoadSessionResponse, McpCapabilities, McpServer, NewSessionRequest,
    NewSessionResponse, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus, PromptCapabilities,
    PromptRequest, PromptResponse, SessionId, SessionNotification, SessionUpdate,
    SetSessionModeRequest, SetSessionModeResponse, StopReason, TextContent, ToolCall,
    ToolCallContent, ToolCallId, ToolCallLocation, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind, V1,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::error;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpConversation {
    messages: Vec<AmpMessage>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AmpStreamEvent {
    System {
        subtype: String,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    User {
        message: AmpMessage,
        #[serde(rename = "parent_tool_use_id")]
        parent_tool_use_id: Option<String>,
        session_id: String,
    },
    Assistant {
        message: serde_json::Value,
        #[serde(rename = "parent_tool_use_id")]
        parent_tool_use_id: Option<String>,
        session_id: String,
    },
    Result {
        subtype: String,
        #[serde(flatten)]
        data: serde_json::Value,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpMessage {
    pub role: String,
    pub content: Vec<AmpContentBlock>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AmpEditFileToolCall {
    pub path: String,
    pub old_str: Option<String>,
    pub new_str: String,
}
use serde::{Deserialize, Serialize};

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
    pub name: AmpTool,
    pub input: serde_json::Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpToolResultContentBlock {
    #[serde(rename = "toolUseID")]
    pub tool_use_id: String,
    pub run: serde_json::Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub struct AmpPlanWriteToolCall {
    pub todos: Vec<AmpPlanTodo>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub struct AmpPlanTodo {
    pub id: String,
    pub content: String,
    pub status: AmpPlanTodoStatus,
    pub priority: AmpPlanTodoPriority,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum AmpPlanTodoStatus {
    Completed,
    Todo,
    #[serde(rename = "in-progress")]
    InProgress,
}

impl AmpPlanTodoStatus {
    pub fn to_acp_plan_status(&self) -> PlanEntryStatus {
        match self {
            AmpPlanTodoStatus::Completed => PlanEntryStatus::Completed,
            AmpPlanTodoStatus::Todo => PlanEntryStatus::Pending,
            AmpPlanTodoStatus::InProgress => PlanEntryStatus::InProgress,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum AmpPlanTodoPriority {
    High,
    Medium,
    Low,
}

impl AmpPlanTodoPriority {
    pub fn to_acp_plan_priority(&self) -> PlanEntryPriority {
        match self {
            AmpPlanTodoPriority::High => PlanEntryPriority::High,
            AmpPlanTodoPriority::Medium => PlanEntryPriority::Medium,
            AmpPlanTodoPriority::Low => PlanEntryPriority::Low,
        }
    }
}

impl AmpPlanWriteToolCall {
    pub fn to_acp_plan(&self) -> Plan {
        Plan {
            entries: self
                .todos
                .iter()
                .map(|todo| PlanEntry {
                    content: todo.content.clone(),
                    status: todo.status.clone().to_acp_plan_status(),
                    priority: todo.priority.clone().to_acp_plan_priority(),
                    meta: None,
                })
                .collect(),
            meta: None,
        }
    }
}

pub trait AmpDiff<T> {
    fn diff(&self, other: &T) -> Option<T>;
}

impl AmpDiff<AmpConversation> for AmpConversation {
    fn diff(&self, other: &AmpConversation) -> Option<AmpConversation> {
        let num_diff = other.messages.len() - self.messages.len();
        let messages_diff: Vec<Option<AmpMessage>> = self
            .messages
            .iter()
            .zip(other.messages.iter())
            .map(|(a, b)| a.diff(b))
            .collect();

        let mut f: Vec<AmpMessage> = messages_diff.iter().filter_map(|m| m.clone()).collect();

        if num_diff > 0 {
            //take the last num_diff items from other
            let mut rem: Vec<AmpMessage> = other
                .messages
                .iter()
                .cloned()
                .rev()
                .take(num_diff)
                .collect();
            f.append(&mut rem);
        }
        Some(AmpConversation { messages: f })
    }
}

impl AmpDiff<AmpContentBlock> for AmpContentBlock {
    fn diff(&self, other: &AmpContentBlock) -> Option<AmpContentBlock> {
        match (self, other) {
            (AmpContentBlock::Text(a), AmpContentBlock::Text(b)) => {
                if a.text == b.text {
                    None
                } else {
                    Some(AmpContentBlock::Text(AmpTextContentBlock {
                        text: b.text.replace(&a.text, ""),
                    }))
                }
            }
            (AmpContentBlock::Thinking(a), AmpContentBlock::Thinking(b)) => {
                if a.thinking == b.thinking {
                    None
                } else {
                    Some(AmpContentBlock::Thinking(AmpThinkingContentBlock {
                        thinking: b.thinking.replace(&a.thinking, ""),
                    }))
                }
            }
            (AmpContentBlock::ToolUse(a), AmpContentBlock::ToolUse(b)) => {
                if a.id == b.id && a.name == b.name && a.input == b.input {
                    None
                } else {
                    Some(AmpContentBlock::ToolUse(AmpToolUseContentBlock {
                        id: b.id.clone(),
                        name: b.name.clone(),
                        input: b.input.clone(),
                    }))
                }
            }
            _ => None,
        }
    }
}

impl AmpDiff<AmpMessage> for AmpMessage {
    fn diff(&self, other: &AmpMessage) -> Option<AmpMessage> {
        let num_diff = other.content.len() - self.content.len();
        if self.role == other.role {
            let mut content_diff: Vec<AmpContentBlock> = self
                .content
                .iter()
                .zip(other.content.iter())
                .filter_map(|(a, b)| a.diff(b))
                .collect();

            if num_diff > 0 {
                //take the last num_diff items from other
                let mut rem: Vec<AmpContentBlock> =
                    other.content.iter().cloned().rev().take(num_diff).collect();
                content_diff.append(&mut rem);
            }
            Some(AmpMessage {
                role: self.role.clone(),
                content: content_diff,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmpTool {
    Bash,
    #[serde(rename = "create_file")]
    CreateFile,
    #[serde(rename = "edit_file")]
    EditFile,
    #[serde(rename = "finder")]
    Finder,
    #[serde(rename = "glob")]
    Glob,
    Grep,
    #[serde(rename = "mermaid")]
    Mermaid,
    #[serde(rename = "oracle")]
    Oracle,
    Read,
    #[serde(rename = "read_mcp_resource")]
    ReadMcpResource,
    #[serde(rename = "read_web_page")]
    ReadWebPage,
    Task,
    #[serde(rename = "todo_read")]
    TodoRead,
    #[serde(rename = "todo_write")]
    TodoWrite,
    #[serde(rename = "undo_edit")]
    UndoEdit,
    #[serde(rename = "web_search")]
    WebSearch,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmpReadToolInput {
    path: String,
    read_range: Option<Vec<i32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmpCreateToolInput {
    path: String,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmpBashToolInput {
    cmd: String,
    cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmpWebSearchToolInput {
    query: String,
    max_results: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmpWebReadToolInput {
    url: String,
    prompt: Option<String>,
    raw: Option<bool>,
}

impl std::fmt::Display for AmpTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AmpTool::Oracle => "Consulting the Oracle",
            AmpTool::Read => "Reading file",
            AmpTool::ReadMcpResource => "Read mcp resource",
            AmpTool::ReadWebPage => "Read webpage",
            AmpTool::Task => "Task",
            AmpTool::TodoRead => "Todo read",
            AmpTool::TodoWrite => "Todo write",
            AmpTool::UndoEdit => "Undo edit",
            AmpTool::WebSearch => "Web search",
            AmpTool::Other => "Unknown",
            AmpTool::Bash => "Bash",
            AmpTool::CreateFile => "Creating file",
            AmpTool::EditFile => "Editing file",
            AmpTool::Finder => "Finder",
            AmpTool::Glob => "Glob",
            AmpTool::Grep => "Grep",
            AmpTool::Mermaid => "Mermaid",
        };
        write!(f, "{s}")
    }
}

fn amp_tool_to_tool_kind(amp_tool: &AmpTool) -> ToolKind {
    match amp_tool {
        AmpTool::Bash => ToolKind::Execute,
        AmpTool::CreateFile => ToolKind::Edit,
        AmpTool::EditFile => ToolKind::Edit,
        AmpTool::Finder => ToolKind::Search,
        AmpTool::Glob => ToolKind::Execute,
        AmpTool::Grep => ToolKind::Execute,
        AmpTool::Mermaid => ToolKind::Other,
        AmpTool::Oracle => ToolKind::Think,
        AmpTool::Read => ToolKind::Read,
        AmpTool::ReadMcpResource => ToolKind::Fetch,
        AmpTool::ReadWebPage => ToolKind::Fetch,
        AmpTool::Task => ToolKind::Think,
        AmpTool::TodoRead => ToolKind::Think,
        AmpTool::TodoWrite => ToolKind::Think,
        AmpTool::UndoEdit => ToolKind::Edit,
        AmpTool::WebSearch => ToolKind::Search,
        AmpTool::Other => ToolKind::Other,
    }
}

pub struct AmpAgent {
    cwd: Rc<RefCell<Option<PathBuf>>>,
    client: OnceCell<Rc<AgentSideConnection>>,
    amp_command: Rc<RefCell<Option<Child>>>,
}

impl AmpAgent {
    pub fn new() -> Self {
        Self {
            cwd: Rc::new(RefCell::new(None)),
            client: OnceCell::new(),
            amp_command: Rc::new(RefCell::new(None)),
        }
    }

    pub fn set_client(&self, client: Rc<AgentSideConnection>) {
        let _ = self.client.set(client);
    }

    pub fn set_amp_command(&self, command: Child) {
        self.amp_command.replace(Some(command));
    }

    pub fn client(&self) -> Rc<AgentSideConnection> {
        Rc::clone(self.client.get().expect("Client should be set"))
    }

    fn get_amp_settings_path(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            let appdata = env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(appdata).join("amp").join("settings.json")
        } else {
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join(".config")
                .join("amp")
                .join("settings.json")
        }
    }

    fn write_mcp_servers(
        &self,
        settings_path: &PathBuf,
        servers: &[McpServer],
    ) -> Result<(), std::io::Error> {
        let mut config = if settings_path.exists() {
            let contents = std::fs::read_to_string(settings_path)?;
            serde_json::from_str::<serde_json::Value>(&contents)
                .unwrap_or_else(|_| serde_json::json!({}))
        } else {
            if let Some(parent) = settings_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            serde_json::json!({})
        };

        let mcp_servers = servers
            .iter()
            .map(|server| match server {
                McpServer::Stdio {
                    name,
                    command,
                    args,
                    env,
                } => {
                    let env_obj: serde_json::Map<String, serde_json::Value> = env
                        .iter()
                        .map(|e| (e.name.clone(), serde_json::Value::String(e.value.clone())))
                        .collect();

                    (
                        name.clone(),
                        serde_json::json!({
                            "command": command,
                            "args": args,
                            "env": env_obj
                        }),
                    )
                }
                McpServer::Http { name, url, headers } => {
                    let headers_obj: serde_json::Map<String, serde_json::Value> = headers
                        .iter()
                        .map(|h| (h.name.clone(), serde_json::Value::String(h.value.clone())))
                        .collect();

                    (
                        name.clone(),
                        serde_json::json!({
                            "type": "http",
                            "url": url,
                            "headers": headers_obj
                        }),
                    )
                }
                McpServer::Sse { name, url, headers } => {
                    let headers_obj: serde_json::Map<String, serde_json::Value> = headers
                        .iter()
                        .map(|h| (h.name.clone(), serde_json::Value::String(h.value.clone())))
                        .collect();

                    (
                        name.clone(),
                        serde_json::json!({
                            "type": "sse",
                            "url": url,
                            "headers": headers_obj
                        }),
                    )
                }
            })
            .collect::<serde_json::Map<String, serde_json::Value>>();

        config["amp.mcpServers"] = serde_json::Value::Object(mcp_servers);

        std::fs::write(settings_path, serde_json::to_string_pretty(&config)?)?;
        Ok(())
    }

    fn extract_thinking(text: &str) -> (Vec<String>, Vec<String>) {
        let mut thinking_parts = Vec::new();
        let mut text_parts = Vec::new();
        let mut current_pos = 0;

        while let Some(thinking_start) = text[current_pos..].find("<thinking>") {
            let abs_start = current_pos + thinking_start;

            if abs_start > current_pos {
                text_parts.push(text[current_pos..abs_start].to_string());
            }

            if let Some(thinking_end) = text[abs_start..].find("</thinking>") {
                let abs_end = abs_start + thinking_end;
                let thinking_content = &text[abs_start + 10..abs_end];
                thinking_parts.push(thinking_content.trim().to_string());
                current_pos = abs_end + 11;
            } else {
                break;
            }
        }

        if current_pos < text.len() {
            text_parts.push(text[current_pos..].to_string());
        }

        (thinking_parts, text_parts)
    }

    async fn process_message(
        &self,
        message: &AmpMessage,
        session_id: SessionId,
        file_edits: &mut HashMap<String, AmpEditFileToolCall>,
        parent_tool_use_id: Option<String>,
    ) {
        for block in &message.content {
            match block {
                AmpContentBlock::Text(text_content_block) => {
                    if parent_tool_use_id.is_some() {
                        continue;
                    }

                    let (thinking_parts, text_parts) =
                        Self::extract_thinking(&text_content_block.text);

                    for thinking in thinking_parts {
                        if !thinking.is_empty() {
                            let notification = SessionNotification {
                                session_id: session_id.clone(),
                                update: SessionUpdate::AgentThoughtChunk {
                                    content: ContentBlock::Text(TextContent {
                                        annotations: None,
                                        text: thinking,
                                        meta: None,
                                    }),
                                },
                                meta: None,
                            };

                            if let Err(e) = self.client().session_notification(notification).await {
                                error!("Failed to send session notification: {:?}", e);
                            }
                        }
                    }

                    for text_part in text_parts {
                        let trimmed = text_part.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let update = if message.role == "user" {
                            SessionUpdate::UserMessageChunk {
                                content: ContentBlock::Text(TextContent {
                                    annotations: None,
                                    text: text_part,
                                    meta: None,
                                }),
                            }
                        } else {
                            SessionUpdate::AgentMessageChunk {
                                content: ContentBlock::Text(TextContent {
                                    annotations: None,
                                    text: text_part,
                                    meta: None,
                                }),
                            }
                        };

                        let notification = SessionNotification {
                            session_id: session_id.clone(),
                            update,
                            meta: None,
                        };

                        if let Err(e) = self.client().session_notification(notification).await {
                            error!("Failed to send session notification: {:?}", e);
                        }
                    }
                }
                AmpContentBlock::Thinking(thinking_content_block) => {
                    let notification = SessionNotification {
                        session_id: session_id.clone(),
                        update: SessionUpdate::AgentThoughtChunk {
                            content: ContentBlock::Text(TextContent {
                                annotations: None,
                                text: thinking_content_block.thinking.clone(),
                                meta: None,
                            }),
                        },
                        meta: None,
                    };

                    if let Err(e) = self.client().session_notification(notification).await {
                        error!("Failed to send session notification: {:?}", e);
                    }
                }
                AmpContentBlock::ToolUse(tool_use_content_block) => {
                    let mut title = tool_use_content_block.name.to_string();
                    let mut content = vec![];
                    let mut meta = None;

                    match tool_use_content_block.name {
                        AmpTool::Oracle => {
                            meta = Some(serde_json::json!({
                                "_isOracle": true,
                                "_modelType": "reasoning",
                                "_description": "Using more powerful reasoning model (GPT-5)"
                            }));
                        }
                        AmpTool::Task => {
                            meta = Some(serde_json::json!({
                                "_isSubagent": true,
                                "_description": "Spawning independent subagent with own context"
                            }));
                        }
                        AmpTool::EditFile => {
                            let data: Result<AmpEditFileToolCall, serde_json::Error> =
                                serde_json::from_value(tool_use_content_block.input.clone());

                            if let Ok(data) = data {
                                file_edits
                                    .entry(tool_use_content_block.id.clone())
                                    .or_insert(data);

                                continue;
                            }
                        }
                        AmpTool::TodoWrite => {
                            let plan: Result<AmpPlanWriteToolCall, serde_json::Error> =
                                serde_json::from_value(tool_use_content_block.input.clone());
                            if let Ok(plan) = plan {
                                let notification = SessionNotification {
                                    session_id: session_id.clone(),
                                    update: SessionUpdate::Plan(plan.to_acp_plan()),
                                    meta: None,
                                };

                                if let Err(e) =
                                    self.client().session_notification(notification).await
                                {
                                    error!("Failed to send session notification: {:?}", e);
                                }
                                continue;
                            }
                        }
                        AmpTool::Read => {
                            let tool_call: Result<AmpReadToolInput, serde_json::Error> =
                                serde_json::from_value(tool_use_content_block.input.clone());

                            if let Ok(t) = tool_call {
                                let path = PathBuf::from(&t.path);
                                let file_name = path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_str()
                                    .unwrap_or_default();
                                if path.is_file() {
                                    title = format!("Read [{}](file://{})", file_name, t.path);
                                } else {
                                    title = format!("Read {}", t.path);
                                }
                            }
                        }
                        AmpTool::CreateFile => {
                            let tool_call: Result<AmpCreateToolInput, serde_json::Error> =
                                serde_json::from_value(tool_use_content_block.input.clone());
                            if let Ok(t) = tool_call {
                                content.push(ToolCallContent::Content {
                                    content: ContentBlock::Text(TextContent {
                                        annotations: None,
                                        text: t.content,
                                        meta: None,
                                    }),
                                });
                                let path = PathBuf::from(&t.path);
                                let file_name = path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_str()
                                    .unwrap_or_default();

                                title = format!("Created [{}](file://{})", file_name, t.path);
                            }
                        }
                        AmpTool::Bash => {
                            let tool_call: Result<AmpBashToolInput, serde_json::Error> =
                                serde_json::from_value(tool_use_content_block.input.clone());

                            if let Ok(t) = tool_call {
                                title = t.cmd;
                            }
                        }
                        AmpTool::WebSearch => {
                            let tool_call: Result<AmpWebSearchToolInput, serde_json::Error> =
                                serde_json::from_value(tool_use_content_block.input.clone());

                            if let Ok(t) = tool_call {
                                title = format!("Searching for \"{}\"", t.query);
                            }
                        }
                        AmpTool::ReadWebPage => {
                            let tool_call: Result<AmpWebReadToolInput, serde_json::Error> =
                                serde_json::from_value(tool_use_content_block.input.clone());

                            if let Ok(t) = tool_call {
                                title = format!("Reading {}", t.url);
                            }
                        }
                        _ => {}
                    }

                    let notification = SessionNotification {
                        session_id: session_id.clone(),
                        update: SessionUpdate::ToolCall(ToolCall {
                            id: ToolCallId(Arc::from(tool_use_content_block.id.clone())),
                            kind: amp_tool_to_tool_kind(&tool_use_content_block.name),
                            status: ToolCallStatus::Pending,
                            title,
                            content,
                            locations: vec![],
                            raw_input: None,
                            raw_output: None,
                            meta,
                        }),
                        meta: None,
                    };

                    if let Err(e) = self.client().session_notification(notification).await {
                        error!("Failed to send session notification: {:?}", e);
                    }
                }
                AmpContentBlock::ToolResult(tool_result_content_block) => {
                    let update;
                    let mut line = None;

                    if let Some(file_edit) =
                        file_edits.remove(&tool_result_content_block.tool_use_id)
                    {
                        if let Some(result) = &tool_result_content_block.run.get("result") {
                            if let Some(diff) = result.get("diff") {
                                if let Some(diff_str) = diff.as_str() {
                                    line = get_line_number_from_diff_str(diff_str);
                                }
                            }
                        }
                        update = ToolCallUpdate {
                            id: ToolCallId(Arc::from(
                                tool_result_content_block.tool_use_id.clone(),
                            )),
                            fields: ToolCallUpdateFields {
                                kind: None,
                                status: Some(ToolCallStatus::Completed),
                                title: None,
                                content: Some(vec![ToolCallContent::Diff {
                                    diff: Diff {
                                        path: PathBuf::from(file_edit.path.clone()),
                                        old_text: file_edit.old_str,
                                        new_text: file_edit.new_str,
                                        meta: None,
                                    },
                                }]),
                                locations: Some(vec![ToolCallLocation {
                                    path: PathBuf::from(file_edit.path.clone()),
                                    line,
                                    meta: None,
                                }]),
                                raw_input: None,
                                raw_output: None,
                            },
                            meta: None,
                        };
                    } else {
                        update = ToolCallUpdate {
                            id: ToolCallId(Arc::from(
                                tool_result_content_block.tool_use_id.clone(),
                            )),
                            fields: ToolCallUpdateFields {
                                content: Some(vec![ToolCallContent::Content {
                                    content: ContentBlock::Text(TextContent {
                                        text: tool_result_content_block.run.to_string(),
                                        annotations: None,
                                        meta: None,
                                    }),
                                }]),
                                kind: None,
                                status: Some(ToolCallStatus::Completed),
                                title: None,
                                locations: None,
                                raw_input: None,
                                raw_output: None,
                            },
                            meta: None,
                        };
                    }

                    if let Err(e) = self
                        .client()
                        .session_notification(SessionNotification {
                            session_id: session_id.clone(),
                            update: SessionUpdate::ToolCallUpdate(update),
                            meta: None,
                        })
                        .await
                    {
                        error!("Failed to send session notification: {:?}", e);
                    }
                }
            }
        }
    }
}

fn get_line_number_from_diff_str(diff: &str) -> Option<u32> {
    let parts = diff.split("@@").collect::<Vec<&str>>();
    let header = parts.get(1)?.trim();
    let line_info_parts = header.split(" ").collect::<Vec<&str>>();
    let final_line_number = line_info_parts.get(1)?;
    let line_number_parts = final_line_number.split(",").collect::<Vec<&str>>();
    let line_number = line_number_parts.first()?.replace("+", "").parse::<u32>();

    line_number.ok()
}

#[async_trait::async_trait(?Send)]
impl Agent for AmpAgent {
    async fn initialize(&self, _request: InitializeRequest) -> Result<InitializeResponse, Error> {
        return Ok(InitializeResponse {
            meta: None,
            protocol_version: V1,
            agent_capabilities: AgentCapabilities {
                load_session: false,
                prompt_capabilities: PromptCapabilities {
                    image: true,
                    audio: true,
                    embedded_context: true,
                    meta: None,
                },
                mcp_capabilities: McpCapabilities {
                    http: true,
                    sse: true,
                    meta: None,
                },
                meta: None,
            },
            auth_methods: vec![AuthMethod {
                id: AuthMethodId(Arc::from("api-key")),
                name: "API Key".to_string(),
                description: Some(
                    "Amp API key (set via AMP_API_KEY env var or run 'amp login')".to_string(),
                ),
                meta: None,
            }],
        });
    }

    async fn authenticate(
        &self,
        _request: AuthenticateRequest,
    ) -> Result<AuthenticateResponse, Error> {
        Ok(AuthenticateResponse { meta: None })
    }

    async fn new_session(&self, request: NewSessionRequest) -> Result<NewSessionResponse, Error> {
        (*self.cwd).borrow_mut().replace(request.cwd.clone());

        Command::new("amp")
            .current_dir(request.cwd.clone())
            .args(["--version"])
            .output()
            .map_err(|_| {
                Error::invalid_request().with_data(
                    "Amp is not installed: curl -fsSL https://ampcode.com/install.sh | bash",
                )
            })?;

        if !request.mcp_servers.is_empty() {
            let settings_path = self.get_amp_settings_path();
            if let Err(e) = self.write_mcp_servers(&settings_path, &request.mcp_servers) {
                error!("Failed to configure MCP servers: {:?}", e);
            }
        }

        let output = Command::new("amp")
            .current_dir(request.cwd.clone())
            .args(["threads", "new"])
            .output()
            .map_err(Error::into_internal_error)?;

        let session_id = match String::from_utf8(output.stdout) {
            Ok(s) => Some(s.replace("\n", "")),
            Err(_) => None,
        };

        if let Some(session_id) = session_id {
            Ok(NewSessionResponse {
                session_id: SessionId(Arc::from(session_id)),
                modes: None,
                meta: None,
            })
        } else {
            Err(Error::internal_error())
        }
    }

    async fn load_session(
        &self,
        _request: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, Error> {
        todo!()
        // Loading sessions is not currently suppored by Zed, the code below should be mostly what is needed to support this.
        // Note: There is an `if message.role != "user" {` that will need to be sorted out as session loading should replay user messages aswell
        //
        // if let Some(conversation) = self.get_amp_thread(request.session_id.clone()) {
        //     self.process_conversation(&conversation, request.session_id)
        //         .await;
        //     todo!()
        // } else {
        //     Err(Error::internal_error().with_data("Could not open amp thread"))
        // }
    }

    async fn prompt(&self, request: PromptRequest) -> Result<PromptResponse, Error> {
        let prompt = request
            .prompt
            .iter()
            .map(|b| match b {
                ContentBlock::Text(text_content) => text_content.text.clone(),
                ContentBlock::Image(_) | ContentBlock::Audio(_) => String::new(),
                ContentBlock::ResourceLink(resource_link) => resource_link.uri.clone(),
                ContentBlock::Resource(embedded_resource) => match &embedded_resource.resource {
                    EmbeddedResourceResource::TextResourceContents(text_resource_contents) => {
                        text_resource_contents.text.clone()
                    }
                    EmbeddedResourceResource::BlobResourceContents(blob_resource_contents) => {
                        blob_resource_contents.blob.clone()
                    }
                },
            })
            .collect::<Vec<String>>()
            .join("");

        let mut child = Command::new("amp")
            .args([
                "threads",
                "continue",
                &request.session_id.0,
                "-x",
                "--stream-json",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| Error::internal_error().with_data("Failed to start amp"))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).map_err(|e| {
                Error::internal_error().with_data(format!("Failed to send prompt to amp: {e}"))
            })?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::internal_error().with_data("Failed to capture stdout"))?;

        self.set_amp_command(child);

        let session_id = request.session_id;
        let reader = BufReader::new(stdout);

        let mut file_edits: HashMap<String, AmpEditFileToolCall> = HashMap::new();

        for line in reader.lines() {
            let line = line.map_err(|e| {
                Error::internal_error().with_data(format!("Failed to read stdout: {e}"))
            })?;

            if line.trim().is_empty() {
                continue;
            }

            let event: AmpStreamEvent = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(e) => {
                    error!("Failed to parse stream event: {e} (line: {line})");
                    continue;
                }
            };

            match event {
                AmpStreamEvent::System { .. } => {
                    continue;
                }
                AmpStreamEvent::User {
                    message,
                    parent_tool_use_id,
                    ..
                } => {
                    self.process_message(
                        &message,
                        session_id.clone(),
                        &mut file_edits,
                        parent_tool_use_id,
                    )
                    .await;
                }
                AmpStreamEvent::Assistant {
                    message,
                    parent_tool_use_id,
                    ..
                } => {
                    if let Ok(amp_message) = serde_json::from_value::<AmpMessage>(message) {
                        self.process_message(
                            &amp_message,
                            session_id.clone(),
                            &mut file_edits,
                            parent_tool_use_id,
                        )
                        .await;
                    }
                }
                AmpStreamEvent::Result { .. } => {
                    break;
                }
            }
        }

        let status = (*self.amp_command)
            .borrow_mut()
            .as_mut()
            .ok_or_else(|| Error::internal_error().with_data("Amp process not found"))?
            .wait()
            .map_err(|e| {
                Error::internal_error().with_data(format!("Failed to wait for amp process: {e}"))
            })?;

        if !status.success() {
            return Err(Error::internal_error()
                .with_data(format!("Amp process exited with status: {status}")));
        }

        Ok(PromptResponse {
            stop_reason: StopReason::EndTurn,
            meta: None,
        })
    }

    async fn cancel(&self, _args: CancelNotification) -> Result<(), Error> {
        let res = (*self.amp_command).borrow_mut().as_mut().unwrap().kill();
        if res.is_err() {
            return Err(Error::internal_error().with_data("Could not kill the amp process"));
        }
        Ok(())
    }

    async fn set_session_mode(
        &self,
        _args: SetSessionModeRequest,
    ) -> Result<SetSessionModeResponse, Error> {
        todo!()
    }

    async fn ext_method(&self, _args: ExtRequest) -> Result<ExtResponse, Error> {
        Err(Error::method_not_found())
    }

    async fn ext_notification(&self, _args: ExtNotification) -> Result<(), Error> {
        Err(Error::method_not_found())
    }
}
