#![allow(dead_code)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    Text,
    Markdown,
    Code,
    Document,
    Checklist,
    Kanban,
    Table,
}

impl std::fmt::Display for NoteKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NoteKind::Text => "text",
            NoteKind::Markdown => "markdown",
            NoteKind::Code => "code",
            NoteKind::Document => "document",
            NoteKind::Checklist => "checklist",
            NoteKind::Kanban => "kanban",
            NoteKind::Table => "table",
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
            "code" => Ok(NoteKind::Code),
            "document" => Ok(NoteKind::Document),
            "checklist" => Ok(NoteKind::Checklist),
            "kanban" => Ok(NoteKind::Kanban),
            "table" => Ok(NoteKind::Table),
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
    pub content_hint: Option<String>,
    pub pinned: bool,
    pub bg_color: Option<String>,
    pub bg_image: Option<String>,
    pub show_preview: bool,
    pub preview_text: Option<String>,
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
    pub pinned: bool,
    pub bg_color: Option<String>,
    pub bg_image: Option<String>,
    pub show_preview: bool,
}

/// Input for note_create / note_update.
#[derive(Debug, Deserialize)]
pub struct NoteInput {
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
    pub tags: Vec<String>,
    pub content_hint: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub bg_color: Option<String>,
    #[serde(default)]
    pub bg_image: Option<String>,
    #[serde(default)]
    pub show_preview: Option<bool>,
}
