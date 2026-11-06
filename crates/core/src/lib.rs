//! Shared domain types for termden.
//!
//! This crate deliberately has no GPUI dependency: the daemon, the control CLI
//! and the UI all speak the same model.

pub mod config;
pub mod layout;
pub mod model;
pub mod state;

pub use config::{Config, ThemeId};
pub use layout::{Axis, DropZone, LayoutNode};
pub use model::{
    Activity, Choice, ChoiceOption, Harness, HarnessId, HarnessKind, HarnessStatus, Mail, Subagent,
    SubagentId, Workspace, WorkspaceId,
};
pub use state::AppState;
