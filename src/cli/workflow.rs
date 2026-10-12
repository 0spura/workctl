use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct WorkflowArgs {
    #[command(subcommand)]
    pub action: WorkflowAction,
}

#[derive(Debug, Subcommand)]
pub enum WorkflowAction {
    /// Approve the current shared workflow rules for this repository
    Approve,
    /// Show the current local approval state
    Status,
    /// List a bounded number of stored workflow actions
    Actions {
        /// Maximum actions to list (1–100)
        #[arg(long, default_value = "50", value_parser = clap::value_parser!(u8).range(1..=100))]
        limit: u8,
    },
    /// Resolve an uncertain action or explicitly authorize its retry
    Resolve {
        /// Stored workflow-action identifier
        id: String,
        /// Record that the provider-side effect is already applied
        #[arg(long, required_unless_present = "retry")]
        applied: bool,
        /// Explicitly authorize reapplying the action
        #[arg(long, required_unless_present = "applied")]
        retry: bool,
    },
}
