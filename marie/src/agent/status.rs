use shared::id::ID;

#[derive(Default, Clone, Copy)]
pub enum AgentStatus {
    #[default]
    Initial,
    Paused,
    Running,
    Failed,
    Yielding(YieldStatus),
    Finished
}

#[derive(Clone, Copy)]
pub enum YieldStatus {
    WaitingToolReply {
        tool_call_id: ID
    },
    RunExhausted
}