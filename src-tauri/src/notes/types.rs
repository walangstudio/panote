#![allow(dead_code)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    Text,
    Markdown,
    Checklist,
    Code,
    Kanban,
}

impl std::fmt::Display for NoteKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NoteKind::Text => "text",
            NoteKind::Markdown => "markdown",
            NoteKind::Checklist => "checklist",
            NoteKind::Code => "code",
            NoteKind::Kanban => "kanban",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for NoteKind {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "text" => Ok(NoteKind::Text),
            "markdown" => Ok(NoteKind::Markdown),
            "checklist" => Ok(NoteKind::Checklist),
            "code" => Ok(NoteKind::Code),
            "kanban" => Ok(NoteKind::Kanban),
            _ => anyhow::bail!("unknown note kind: {s}"),
        }
    }
}

// ----- Content shapes (serialized to JSON, then encrypted) -----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownContent {
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItem {
    pub id: String,
    pub text: String,
    pub checked: bool,
    pub children: Vec<CheckItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistContent {
    pub items: Vec<CheckItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContent {
    pub lang: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanCard {
    pub id: String,
    pub title: String,
    pub note_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanColumn {
    pub id: String,
    pub name: String,
    pub cards: Vec<KanbanCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanContent {
    pub columns: Vec<KanbanColumn>,
}

// ----- API types (sent to/from frontend) -----

/// Lightweight metadata returned by note_list (no decrypted content).
#[derive(Debug, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub has_note_password: bool,
}

/// Full note returned by note_get.
#[derive(Debug, Serialize, Deserialize)]
pub struct NoteDetail {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Input for note_create / note_update.
#[derive(Debug, Deserialize)]
pub struct NoteInput {
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
    pub tags: Vec<String>,
}
