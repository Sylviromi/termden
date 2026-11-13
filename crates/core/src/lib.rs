//! Shared domain types for termden.
//!
//! This crate deliberately has no GPUI dependency: the daemon, the control CLI
//! and the UI all speak the same model.

pub mod config;
pub mod layout;
pub mod model;
pub mod state;

pub use config::{Config, ThemeId, TitleBar};
pub use layout::{Axis, DropZone, LayoutNode};
pub use model::{
    AGENT_COLORS, Activity, Choice, ChoiceOption, Harness, HarnessId, HarnessKind, HarnessStatus,
    Mail, ShowItem, Showcase, Subagent, SubagentId, Workspace, WorkspaceId, assign_agent_colors,
};
pub use state::AppState;
