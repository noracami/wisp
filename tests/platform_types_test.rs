use wisp::platform::{Platform, ChatRequest, ChatResponse, ChatMessage};
use uuid::Uuid;

#[test]
fn platform_to_db_string() {
    assert_eq!(Platform::Discord.as_str(), "discord");
    assert_eq!(Platform::Line.as_str(), "line");
}

#[test]
fn platform_from_db_string() {
    assert_eq!(Platform::from_str("discord"), Some(Platform::Discord));
    assert_eq!(Platform::from_str("line"), Some(Platform::Line));
    assert_eq!(Platform::from_str("unknown"), None);
}

#[test]
fn chat_request_construction() {
    let req = ChatRequest {
        user_id: Uuid::new_v4(),
        channel_id: "ch-123".to_string(),
        platform: Platform::Discord,
        message: "hello".to_string(),
    };
    assert_eq!(req.message, "hello");
    assert_eq!(req.platform.as_str(), "discord");
}

#[test]
fn chat_response_construction() {
    let resp = ChatResponse {
        text: "hi there".to_string(),
    };
    assert_eq!(resp.text, "hi there");
}

#[test]
fn chat_message_construction() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
    };
    assert_eq!(msg.role, "user");
}
