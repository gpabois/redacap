#[derive(Default)]
pub enum AgentStatus {
    #[default]
    Initial,
    Running,
    Failed,
    Yielding,
    Finished
}