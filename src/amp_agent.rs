use agent_client_protocol::{
    Agent, AgentCapabilities, AgentSideConnection, Annotations, AudioContent, AuthenticateRequest,
    AuthenticateResponse, BlobResourceContents, CancelNotification, Client, ContentBlock, Diff,
    EmbeddedResource, EmbeddedResourceResource, Error, ErrorCode, ExtNotification, ExtRequest,
    ExtResponse, ImageContent, InitializeRequest, InitializeResponse, LoadSessionRequest,
    LoadSessionResponse, McpCapabilities, McpServer, NewSessionRequest, NewSessionResponse,
    PermissionOption, PermissionOptionId, PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority,
    PlanEntryStatus, PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, ResourceLink, SessionId, SessionMode, SessionModeId,
    SessionModeState, SessionNotification, SessionUpdate, SetSessionModeRequest,
    SetSessionModeResponse, StopReason, TextContent, TextResourceContents, ToolCall,
    ToolCallContent, ToolCallId, ToolCallLocation, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind, V1,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::time::sleep;
use tracing::error;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpConversation {
    messages: Vec<AmpMessage>,
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
    pub old_str: String,
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

fn amp_tool_to_tool_kind(amp_tool: &str) -> ToolKind {
    match amp_tool {
        "Bash" => ToolKind::Execute,
        "create_file" => ToolKind::Edit,
        "edit_file" => ToolKind::Edit,
        "finder" => ToolKind::Search,
        "glob" => ToolKind::Execute,
        "Grep" => ToolKind::Execute,
        "mermaid" => ToolKind::Other,
        "oracle" => ToolKind::Think,
        "Read" => ToolKind::Read,
        "read_mcp_resource" => ToolKind::Fetch,
        "read_web_page" => ToolKind::Fetch,
        "Task" => ToolKind::Think,
        "todo_read" => ToolKind::Think,
        "todo_write" => ToolKind::Think,
        "undo_edit" => ToolKind::Edit,
        "web_search" => ToolKind::Search,
        _ => ToolKind::Other,
    }
}

pub struct AmpAgent {
    cwd: Rc<RefCell<Option<PathBuf>>>,
    client: OnceCell<Rc<AgentSideConnection>>,
    amp_command: Rc<RefCell<Option<Child>>>,
    threads_directory: PathBuf,
}

impl AmpAgent {
    pub fn new() -> Self {
        // Todo: Windows support
        let home_dir = env::home_dir().unwrap();
        let threads_directory = format!("{}/.local/share/amp/threads/", home_dir.display());

        Self {
            cwd: Rc::new(RefCell::new(None)),
            client: OnceCell::new(),
            amp_command: Rc::new(RefCell::new(None)),
            threads_directory: PathBuf::from(threads_directory),
        }
    }

    pub fn set_client(&self, client: Rc<AgentSideConnection>) {
        self.client.set(client);
    }

    pub fn set_amp_command(&self, command: Child) {
        self.amp_command.replace(Some(command));
    }

    pub fn client(&self) -> Rc<AgentSideConnection> {
        Rc::clone(self.client.get().expect("Client should be set"))
    }

    pub fn get_amp_thread(&self, thread_id: SessionId) -> Option<AmpConversation> {
        let thread_id_str: &str = &thread_id.0;
        let thread_path = self
            .threads_directory
            .join(format!("{}.json", thread_id_str));

        let mut file = File::open(&thread_path).expect("Failed to open amp thread file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("Failed to read amp thread file");

        serde_json::from_str(&contents).ok()
    }

    async fn process_conversation(&self, conversation: &AmpConversation, session_id: SessionId) {
        let mut file_edits: HashMap<String, AmpEditFileToolCall> = HashMap::new();
        for message in &conversation.messages {
            for block in &message.content {
                match block {
                    AmpContentBlock::Text(text_content_block) => {
                        if message.role != "user" {
                            let notification = SessionNotification {
                                session_id: session_id.clone(),
                                update: SessionUpdate::AgentMessageChunk {
                                    content: ContentBlock::Text(TextContent {
                                        annotations: None,
                                        text: text_content_block.text.clone(),
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
                        match tool_use_content_block.name.as_str() {
                            "edit_file" => {
                                dbg!("edit file");
                                dbg!(&tool_use_content_block);
                                let data: Result<AmpEditFileToolCall, serde_json::Error> =
                                    serde_json::from_value(tool_use_content_block.input.clone());

                                if let Ok(data) = data {
                                    file_edits.insert(tool_use_content_block.id.clone(), data);
                                }
                            }
                            _ => {
                                // Handle unknown name
                            }
                        }

                        let notification = SessionNotification {
                            session_id: session_id.clone(),
                            update: SessionUpdate::ToolCall(ToolCall {
                                id: ToolCallId(Arc::from(tool_use_content_block.id.clone())),
                                kind: amp_tool_to_tool_kind(tool_use_content_block.name.as_str()),
                                status: ToolCallStatus::Pending,
                                title: tool_use_content_block.name.clone(),
                                content: vec![],
                                locations: vec![],
                                raw_input: None,
                                raw_output: None,

                                meta: None,
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

                        //check if theres a file edit for this tool call
                        if let Some(file_edit) =
                            file_edits.remove(&tool_result_content_block.tool_use_id)
                        {
                            if let Some(result) = &tool_result_content_block.run.get("result") {
                                // Parse the diff to get the line numbers
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
                                            old_text: Some(file_edit.old_str),
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
    async fn initialize(&self, request: InitializeRequest) -> Result<InitializeResponse, Error> {
        return Ok(InitializeResponse {
            meta: None,
            protocol_version: V1,
            agent_capabilities: AgentCapabilities {
                load_session: false,
                prompt_capabilities: PromptCapabilities {
                    image: false,
                    audio: false,
                    embedded_context: false,
                    meta: None,
                },
                mcp_capabilities: McpCapabilities {
                    http: false,
                    sse: false,
                    meta: None,
                },
                meta: None,
            },
            auth_methods: vec![],
        });
    }

    async fn authenticate(
        &self,
        _request: AuthenticateRequest,
    ) -> Result<AuthenticateResponse, Error> {
        // We don't currently require authentication
        Ok(AuthenticateResponse { meta: None })
    }

    async fn new_session(&self, request: NewSessionRequest) -> Result<NewSessionResponse, Error> {
        (*self.cwd).borrow_mut().replace(request.cwd.clone());
        let output = Command::new("amp")
            .current_dir(request.cwd.clone())
            .args(["threads", "new"])
            .output()
            .expect("failed to execute process");

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
        request: LoadSessionRequest,
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
        let output = Command::new("amp")
            .current_dir(self.cwd.borrow().clone().unwrap())
            .args([
                "threads",
                "continue",
                &request.session_id.0,
                "-x",
                request
                    .prompt
                    .iter()
                    .find_map(|b| {
                        if let ContentBlock::Text(t) = b {
                            Some(t)
                        } else {
                            None
                        }
                    })
                    .unwrap()
                    .text
                    .as_str(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .expect("Failed to spawn command");

        self.set_amp_command(output);

        // Implementation note
        // AMP has a json mode but this has some drawbacks
        // 1. Tokens within a message are not streamed
        // 2. No thinking
        // 3. Tool call and result blocks appear to come together rather then one by one
        //
        // Due to this we read the thread file directly and diff the changes. Although this is a more brittle and complicated approach it allows us to get the features laid out above which I believe provides a better user experience

        // We keep track of the state of the conversation so that we can diff it with the new state to know what to send to the acp client
        let mut conversation_so_far: Option<AmpConversation> = None;
        let session_id = request.session_id;
        loop {
            let res = (*self.amp_command)
                .borrow_mut()
                .as_mut()
                .unwrap()
                .try_wait();

            if let Err(e) = res {
                return Err(Error::internal_error());
            } else if let Ok(status) = res {
                let conversation = match self.get_amp_thread(session_id.clone()) {
                    Some(conversation) => conversation,
                    None => return Err(Error::internal_error()),
                };

                if conversation_so_far.is_none() {
                    conversation_so_far = Some(conversation.clone());
                } else if let Some(ref mut prev_conversation) = conversation_so_far {
                    let diff = prev_conversation.diff(&conversation);
                    if let Some(conversation) = diff {
                        self.process_conversation(&conversation, session_id.clone())
                            .await;
                    }
                    conversation_so_far = Some(conversation);

                    if status.is_some() {
                        //finished processing user response
                        // Send a end turn response
                        return Ok(PromptResponse {
                            stop_reason: StopReason::EndTurn,
                            meta: None,
                        });
                    }
                }
            }
            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn cancel(&self, args: CancelNotification) -> Result<(), Error> {
        (*self.amp_command).borrow_mut().as_mut().unwrap().kill();
        Ok(())
    }

    async fn set_session_mode(
        &self,
        args: SetSessionModeRequest,
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
