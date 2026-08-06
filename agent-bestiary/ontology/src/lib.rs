pub mod error;
pub mod git;
pub mod mermaid;
pub mod snapshot;
pub mod types;
pub mod workspace_git;

pub use error::{OntologyError, Result};
pub use git::GitManager;
pub use mermaid::MermaidGenerator;
pub use snapshot::{OntologySnapshot, SnapshotManager};
pub use types::{
    Cardinality, DiagramMetadata, GitCommit, GitConfig, MermaidConfig, MermaidDiagram,
    OntologyStats,
};
pub use workspace_git::{CommitAuthor, FileEntry, WorkspaceCommit, WorkspaceGitManager};
