// JSON-RPC bridge for Amp Agent Control Protocol
// Enables IDE clients to communicate with Amp CLI for thread management and agent interactions

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
pub struct AgentJsonRpcResponse<T> {
    pub jsonrpc: String,
    pub method: JsonRPCResponseMethod,
    pub params: T,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JsonRPCResponseMethod {
    #[serde(rename = "session/update")]
    SessionUpdate,
}

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

//Initialization server response
#[derive(Deserialize, Serialize, Debug)]
pub struct JsonRPCResponse<T> {
    pub jsonrpc: String,
    pub id: u32,
    pub result: T,
}

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
    ToolCall(AgentToolCall),
    ToolCallUpdate(AgentToolCallResult),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCall {
    tool_call_id: String,
    title: String,
    kind: ToolKind,
    status: ToolCallStatus,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResult {
    tool_call_id: String,
    status: ToolCallStatus,
    content: Vec<AgentToolCallResultContent>,
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
    content: ContentBlock,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResultDiffBlock {
    new_text: String,
    old_text: String,
    path: String,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResultFollowBlock {
    path: String,
    line: usize,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub protocol_version: u32,
    pub agent_capabilities: AgentCapabilities,
    pub auth_methods: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EndTurnResponse {
    pub stop_reason: String,
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

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionResponse {
    pub session_id: String,
}

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

//messages
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    pub session_id: String,
    pub prompt: Vec<ContentBlock>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TextContentBlock {
    pub text: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum ContentBlock {
    Text(TextContentBlock),
    Thinking(ThinkingContentBlock),
    ToolUse(ToolUseContentBlock),
    ToolResult(ToolResultContentBlock),
}

impl Diff<ContentBlock> for ContentBlock {
    fn diff(&self, other: &ContentBlock) -> Option<ContentBlock> {
        match (self, other) {
            (ContentBlock::Text(a), ContentBlock::Text(b)) => {
                if a.text == b.text {
                    None
                } else {
                    Some(ContentBlock::Text(TextContentBlock {
                        text: b.text.replace(&a.text, ""),
                    }))
                }
            }
            (ContentBlock::Thinking(a), ContentBlock::Thinking(b)) => {
                if a.thinking == b.thinking {
                    None
                } else {
                    Some(ContentBlock::Thinking(ThinkingContentBlock {
                        thinking: b.thinking.replace(&a.thinking, ""),
                    }))
                }
            }
            (ContentBlock::ToolUse(a), ContentBlock::ToolUse(b)) => {
                if a.id == b.id && a.name == b.name && a.input == b.input {
                    None
                } else {
                    Some(ContentBlock::ToolUse(ToolUseContentBlock {
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingContentBlock {
    pub thinking: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseContentBlock {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultContentBlock {
    #[serde(rename = "toolUseID")]
    pub tool_use_id: String,
    //#[serde(rename(serialize = "run", deserialize = "content"))]
    pub run: serde_json::Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpConversation {
    messages: Vec<AmpMessage>,
}

impl Diff<AmpConversation> for AmpConversation {
    fn diff(&self, other: &AmpConversation) -> Option<AmpConversation> {
        let num_diff = other.messages.len() - self.messages.len();
        assert_eq!(num_diff >= 0, true);
        let messages_diff: Vec<Option<AmpMessage>> = self
            .messages
            .iter()
            .zip(other.messages.iter())
            .map(|(a, b)| a.diff(b))
            .collect();

        let mut f: Vec<AmpMessage> = messages_diff
            .iter()
            .filter(|m| m.is_some())
            .map(|m| m.clone().unwrap())
            .collect();

        if num_diff > 0 {
            //take the last num_diff items from other
            let mut rem: Vec<AmpMessage> = other
                .messages
                .iter()
                .map(|c| c.clone())
                .rev()
                .take(num_diff)
                .collect();
            f.append(&mut rem);
        }
        Some(AmpConversation { messages: f })
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AmpMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

trait Diff<T> {
    fn diff(&self, other: &T) -> Option<T>;
}

impl Diff<AmpMessage> for AmpMessage {
    fn diff(&self, other: &AmpMessage) -> Option<AmpMessage> {
        let num_diff = other.content.len() - self.content.len();
        assert_eq!(num_diff >= 0, true);
        if self.role == other.role {
            let mut content_diff: Vec<ContentBlock> = self
                .content
                .iter()
                .zip(other.content.iter())
                .map(|(a, b)| a.diff(b))
                .filter(|m| m.is_some())
                .map(|m| m.unwrap())
                .collect();

            if num_diff > 0 {
                //take the last num_diff items from other
                let mut rem: Vec<ContentBlock> = other
                    .content
                    .iter()
                    .map(|c| c.clone())
                    .rev()
                    .take(num_diff)
                    .collect();
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct AgentMessageChunk {
    content: ContentBlock,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EditFileToolCall {
    pub path: String,
    pub old_str: String,
    pub new_str: String,
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

impl ToolKind {
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
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    //let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    //let mut writer = BufWriter::new(stdout.lock());

    let mut line = String::new();
    let mut current_working_directory = None;
    let mut session_id = None;
    loop {
        match reader.read_line(&mut line) {
            Ok(0) => {
                // 0 bytes read indicates EOF
                println!("Stdin closed (EOF detected)");
                break;
            }
            Ok(n) => {
                let request: JsonRPCRequest = serde_json::from_str(&line)?;
                match request.call {
                    JsonRPCRequestMethodCall::Initialize(InitializeRequest {
                        protocol_version,
                        client_capabilities,
                    }) => {
                        let res = JsonRPCResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id,
                            result: InitializeResponse {
                                protocol_version: 1,
                                agent_capabilities: AgentCapabilities {
                                    load_session: true,
                                    prompt_capabilities: PromptCapabilities {
                                        image: false,
                                        video: false,
                                        embeded_context: false,
                                    },
                                    mcp: MCP {
                                        http: false,
                                        sse: false,
                                    },
                                },
                                auth_methods: vec![],
                            },
                        };
                        //writer.write(serde_json::to_string(&res)?.as_bytes())?;
                        //writer.flush().unwrap();
                        println!("{}", serde_json::to_string(&res)?);
                        line.clear();
                    }
                    JsonRPCRequestMethodCall::NewSession(NewSessionRequest {
                        cwd,
                        mcp_servers,
                    }) => {
                        // Init finished
                        // Create new amp session
                        // return session_id
                        current_working_directory = Some(cwd);

                        let output = Command::new("amp")
                            .current_dir(current_working_directory.clone().unwrap())
                            .args(["threads", "new"])
                            .output()
                            .expect("failed to execute process");

                        session_id = match String::from_utf8(output.stdout) {
                            Ok(s) => Some(s.replace("\n", "")),
                            Err(_) => None,
                        };

                        let res = JsonRPCResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id,
                            result: NewSessionResponse {
                                session_id: session_id.clone().unwrap(),
                            },
                        };
                        println!("{}", serde_json::to_string(&res)?);
                        line.clear();
                    }
                    JsonRPCRequestMethodCall::Prompt(PromptRequest { session_id, prompt }) => {
                        // send message to thread
                        assert!(current_working_directory.clone().is_some());

                        let mut output = Command::new("amp")
                            .current_dir(current_working_directory.clone().unwrap())
                            .args([
                                "threads",
                                "continue",
                                &session_id.clone(),
                                "-x",
                                &prompt
                                    .iter()
                                    .find_map(|b| {
                                        if let ContentBlock::Text(t) = b {
                                            Some(t)
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap()
                                    .text,
                            ])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .spawn()
                            .expect("Failed to spawn command");

                        // Wait for the process to complete
                        let home_dir = env::home_dir().unwrap();

                        //keep checking the file
                        let thread_path = format!(
                            "{}/.local/share/amp/threads/{}.json",
                            home_dir.display(),
                            &session_id.clone()
                        );

                        let mut file_edits: HashMap<String, EditFileToolCall> = HashMap::new();

                        let mut conversation_so_far: Option<AmpConversation> = None;

                        loop {
                            let res = output.try_wait();

                            if let Err(e) = res {
                                eprintln!("Error waiting for command: {}", e);
                                break;
                            } else if let Ok(status) = res {
                                let mut file = File::open(&thread_path)?;
                                let mut contents = String::new();
                                file.read_to_string(&mut contents)?;

                                let conversation: AmpConversation =
                                    serde_json::from_str(&contents)?;

                                if conversation_so_far.is_none() {
                                    conversation_so_far = Some(conversation.clone());
                                } else if let Some(ref mut prev_conversation) = conversation_so_far
                                {
                                    let diff = prev_conversation.diff(&conversation);

                                    if let Some(conversation) = diff {
                                        for message in conversation.messages {
                                            for block in message.content {
                                                match block {
                                                    ContentBlock::Text(text_content_block) => {
                                                        if message.role != "user" {
                                                            let response = AgentJsonRpcResponse {
                                                            jsonrpc: String::from("2.0"),
                                                            method: JsonRPCResponseMethod::SessionUpdate,
                                                            params: SessionUpdateResponse {
                                                                session_id: session_id.clone(),
                                                                update: SessionUpdate::AgentMessageChunk(
                                                                    AgentMessageChunk {
                                                                        content: ContentBlock::Text(
                                                                          text_content_block
                                                                        ),
                                                                    },
                                                                ),
                                                            },
                                                        };
                                                            println!(
                                                                "{}",
                                                                serde_json::to_string(&response)?
                                                            );
                                                        }
                                                    }
                                                    ContentBlock::Thinking(
                                                        thinking_content_block,
                                                    ) => {
                                                        //       let response = AgentJsonRpcResponse {
                                                        //     jsonrpc: String::from("2.0"),
                                                        //     method: JsonRPCResponseMethod::SessionUpdate,
                                                        //     params: SessionUpdateResponse {
                                                        //         session_id: session_id.clone(),
                                                        //         update: SessionUpdate::AgentMessageChunk(
                                                        //             AgentMessageChunk {
                                                        //                 content: ContentBlock::Text(TextContentBlock { text: thinking_content_block.thinking }
                                                        //                 ),
                                                        //             },
                                                        //         ),
                                                        //     },
                                                        // };
                                                        //       println!(
                                                        //           "{}",
                                                        //           serde_json::to_string(&response)?
                                                        //       );
                                                    }
                                                    ContentBlock::ToolUse(
                                                        tool_use_content_block,
                                                    ) => {
                                                        match tool_use_content_block.name.as_str() {
                                                            "edit_file" => {
                                                                dbg!("edit file");
                                                                dbg!(&tool_use_content_block);
                                                                let data: Result<
                                                                    EditFileToolCall,
                                                                    serde_json::Error,
                                                                > = serde_json::from_value(
                                                                    tool_use_content_block.input,
                                                                );

                                                                if let Ok(data) = data {
                                                                    file_edits.insert(
                                                                        tool_use_content_block
                                                                            .id
                                                                            .clone(),
                                                                        data,
                                                                    );
                                                                }
                                                            }
                                                            _ => {
                                                                // Handle unknown name
                                                            }
                                                        }
                                                        let response = AgentJsonRpcResponse {
                                                            jsonrpc: String::from("2.0"),
                                                            method:
                                                                JsonRPCResponseMethod::SessionUpdate,
                                                            params: SessionUpdateResponse {
                                                                session_id: session_id.clone(),
                                                                update: SessionUpdate::ToolCall(
                                                                    AgentToolCall {
                                                                        tool_call_id:
                                                                            tool_use_content_block
                                                                                .id,
                                                                        title:
                                                                            tool_use_content_block
                                                                                .name.clone(),
                                                                        kind: ToolKind::amp_tool_to_tool_kind(tool_use_content_block
                                                                            .name.as_str()),
                                                                        status:
                                                                            ToolCallStatus::Pending,
                                                                    },
                                                                ),
                                                            },
                                                        };
                                                        println!(
                                                            "{}",
                                                            serde_json::to_string(&response)?
                                                        );
                                                    }
                                                    ContentBlock::ToolResult(
                                                        tool_result_content_block,
                                                    ) => {
                                                        //check if theres a file edit for this
                                                        let update;
                                                        if let Some(file_edit) = file_edits.remove(
                                                            &tool_result_content_block.tool_use_id,
                                                        ) {
                                                            let mut tool_call_result =
                                                            AgentToolCallResult {
                                                              tool_call_id: tool_result_content_block.tool_use_id,
                                                              status: ToolCallStatus::Completed,
                                                              content: vec![
                                                                AgentToolCallResultContent::Diff(AgentToolCallResultDiffBlock { new_text: file_edit.new_str, old_text: file_edit.old_str, path: file_edit.path.clone() })]
                                                                };

                                                            //extract line info
                                                            if let Some(result) =
                                                                &tool_result_content_block
                                                                    .run
                                                                    .get("result")
                                                            {
                                                                if let Some(diff) =
                                                                    result.get("diff")
                                                                {
                                                                    let lines = diff
                                                                        .as_str()
                                                                        .unwrap()
                                                                        .split("@@")
                                                                        .collect::<Vec<&str>>();

                                                                    let line = lines
                                                                        .get(1)
                                                                        .unwrap()
                                                                        .trim()
                                                                        .split(" ")
                                                                        .collect::<Vec<&str>>()
                                                                        .get(1)
                                                                        .unwrap()
                                                                        .split(",")
                                                                        .collect::<Vec<&str>>()
                                                                        .get(0)
                                                                        .unwrap()
                                                                        .replace("+", "");
                                                                    let t = AgentToolCallResultContent::Follow(AgentToolCallResultFollowBlock { path: file_edit.path, line: line.parse().unwrap()});
                                                                    tool_call_result
                                                                        .content
                                                                        .push(t);
                                                                }
                                                            }
                                                            update = SessionUpdate::ToolCallUpdate(
                                                                tool_call_result,
                                                            );
                                                        } else {
                                                            update = SessionUpdate::ToolCallUpdate(
                                                              AgentToolCallResult {
                                                                tool_call_id: tool_result_content_block.tool_use_id,
                                                                status: ToolCallStatus::Completed,
                                                                content: vec![
                                                                  AgentToolCallResultContent::Content(AgentToolCallResultContentBlock {
                                                                    content: ContentBlock::Text(
                                                                      TextContentBlock {
                                                                        text: tool_result_content_block.run.to_string()
                                                                      })
                                                                  })]
                                                              },
                                                          );
                                                        }

                                                        let response = AgentJsonRpcResponse {
                                                            jsonrpc: String::from("2.0"),
                                                            method:
                                                                JsonRPCResponseMethod::SessionUpdate,
                                                            params: SessionUpdateResponse {
                                                                session_id: session_id.clone(),
                                                                update,
                                                            },
                                                        };
                                                        println!(
                                                            "{}",
                                                            serde_json::to_string(&response)?
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    //println!("Diff: {:?}", diff);
                                    conversation_so_far = Some(conversation);

                                    if let Some(_) = status {
                                        //finished processing user response
                                        // Send a end turn response
                                        let res = JsonRPCResponse {
                                            jsonrpc: "2.0".to_string(),
                                            id: request.id,
                                            result: EndTurnResponse {
                                                stop_reason: "end_turn".to_string(),
                                            },
                                        };
                                        println!("{}", serde_json::to_string(&res)?);
                                        break;
                                    }
                                }
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }

                        line.clear();
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading from stdin: {}", e);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn diff_text_content_blocks() {
        let a = AmpMessage {
            role: String::from("assistant"),
            content: vec![ContentBlock::Text(TextContentBlock {
                text: String::from("aa"),
            })],
        };

        let b = AmpMessage {
            role: String::from("assistant"),
            content: vec![ContentBlock::Text(TextContentBlock {
                text: String::from("Hey, how are you?"),
            })],
        };

        let diff = a.diff(&b);

        assert!(diff.is_some());
        dbg!(diff);
        panic!()
    }

    #[test]
    fn diff_conversation() {
        let a = AmpConversation {
            messages: vec![
                AmpMessage {
                    role: String::from("assistant"),
                    content: vec![
                        ContentBlock::Thinking(ThinkingContentBlock {
                            thinking: String::from(""),
                        }),
                        ContentBlock::Text(TextContentBlock {
                            text: String::from(""),
                        }),
                    ],
                },
                AmpMessage {
                    role: String::from("assistant"),
                    content: vec![
                        ContentBlock::Thinking(ThinkingContentBlock {
                            thinking: String::from(""),
                        }),
                        ContentBlock::Text(TextContentBlock {
                            text: String::from(""),
                        }),
                    ],
                },
            ],
        };

        let b = AmpConversation {
            messages: vec![
                AmpMessage {
                    role: String::from("assistant"),
                    content: vec![
                        ContentBlock::Thinking(ThinkingContentBlock {
                            thinking: String::from("i am thinking alot"),
                        }),
                        ContentBlock::Text(TextContentBlock {
                            text: String::from("hey"),
                        }),
                    ],
                },
                AmpMessage {
                    role: String::from("assistant"),
                    content: vec![
                        ContentBlock::Thinking(ThinkingContentBlock {
                            thinking: String::from("wwwwww"),
                        }),
                        ContentBlock::Text(TextContentBlock {
                            text: String::from(".com"),
                        }),
                    ],
                },
            ],
        };

        let diff = a.diff(&b);

        assert!(diff.is_some());
        dbg!(diff);
        panic!()
    }

    #[derive(Debug)]
    struct diff {
        old_text: String,
        new_text: String,
        old_line: usize,
        new_line: usize,
    }

    impl diff {
        fn new(diff: &str) -> Self {
            let lines = diff.split("@@").collect::<Vec<&str>>();
            let line_number_info = lines.get(1).unwrap().trim();
            todo!()
        }
    }

    fn code_diff() {
        let diff = "```diff\nIndex: /Users/hamishtaylor/dev/my-amp-acp/src/main.rs\n===================================================================\n--- /Users/hamishtaylor/dev/my-amp-acp/src/main.rs\toriginal\n+++ /Users/hamishtaylor/dev/my-amp-acp/src/main.rs\tmodified\n@@ -363,8 +363,9 @@\n     content: ContentBlock,\n }\n \n // Main entry point for the ACP bridge\n+// This is the main entry point that handles JSON-RPC communication for Amp Agent Control Protocol\n fn main() -> io::Result<()> {\n     let stdin = io::stdin();\n     //let stdout = io::stdout();\n     let mut reader = BufReader::new(stdin.lock());\n```";

        let diff = diff::new(diff);
        println!("{:?}", diff);
    }
}
