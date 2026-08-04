use crate::{Actor, ApprovalPolicy, MessageRole};

pub(crate) fn message_role(value: &str) -> MessageRole {
    match value {
        "system" => MessageRole::System,
        "developer" => MessageRole::Developer,
        "user" => MessageRole::User,
        "assistant" | "agent" => MessageRole::Assistant,
        "tool" => MessageRole::Tool,
        value => MessageRole::Other(value.to_owned()),
    }
}

pub(crate) fn actor_for_message(role: &MessageRole) -> Actor {
    match role {
        MessageRole::System | MessageRole::Developer => Actor::System,
        MessageRole::User => Actor::User,
        MessageRole::Assistant => Actor::Agent,
        MessageRole::Tool => Actor::Tool,
        MessageRole::Other(value) => Actor::Other(value.clone()),
    }
}

pub(crate) fn approval_policy(value: &str) -> ApprovalPolicy {
    match value {
        "untrusted" => ApprovalPolicy::Untrusted,
        "on_request" | "on-request" | "ask" | "default" => ApprovalPolicy::OnRequest,
        "on_failure" | "on-failure" => ApprovalPolicy::OnFailure,
        "never" | "bypass_permissions" | "bypassPermissions" => ApprovalPolicy::Never,
        value => ApprovalPolicy::Other(value.to_owned()),
    }
}
