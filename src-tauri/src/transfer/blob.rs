use serde::{Deserialize, Serialize};

/// Plaintext note payload sent over the encrypted transport (TLS / BLE).
/// Protected by transport security — no note keys are included.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferBlob {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TransferBlob {
    pub fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> TransferBlob {
        TransferBlob {
            id: "test-uuid-1234".into(),
            kind: "markdown".into(),
            title: "My secret note".into(),
            content: json!({ "body": "Hello **world**" }),
            tags: vec!["rust".into(), "notes".into()],
            created_at: 1700000000,
            updated_at: 1700000001,
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let blob = sample();
        let bytes = blob.encode().unwrap();
        let recovered = TransferBlob::decode(&bytes).unwrap();
        assert_eq!(blob, recovered);
    }

    #[test]
    fn encode_is_valid_json() {
        let bytes = sample().encode().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["kind"], "markdown");
        assert_eq!(v["title"], "My secret note");
        assert_eq!(v["tags"], json!(["rust", "notes"]));
    }

    #[test]
    fn decode_invalid_bytes_errors() {
        assert!(TransferBlob::decode(b"not json at all!!!").is_err());
    }

    #[test]
    fn decode_missing_field_errors() {
        // Missing 'content'
        let bad = br#"{"id":"x","kind":"markdown","title":"t","tags":[],"created_at":0,"updated_at":0}"#;
        assert!(TransferBlob::decode(bad).is_err());
    }

    #[test]
    fn checklist_content_roundtrip() {
        let blob = TransferBlob {
            kind: "checklist".into(),
            content: json!({
                "items": [
                    { "id": "1", "text": "Buy milk", "checked": false, "children": [] },
                    { "id": "2", "text": "Buy eggs", "checked": true, "children": [] }
                ]
            }),
            ..sample()
        };
        let recovered = TransferBlob::decode(&blob.encode().unwrap()).unwrap();
        assert_eq!(recovered.content["items"][0]["text"], "Buy milk");
        assert_eq!(recovered.content["items"][1]["checked"], true);
    }

    #[test]
    fn kanban_content_roundtrip() {
        let blob = TransferBlob {
            kind: "kanban".into(),
            content: json!({
                "columns": [
                    { "id": "col1", "name": "To do", "cards": [
                        { "id": "c1", "title": "Task A", "note_ref": null }
                    ]}
                ]
            }),
            ..sample()
        };
        let recovered = TransferBlob::decode(&blob.encode().unwrap()).unwrap();
        assert_eq!(recovered.content["columns"][0]["name"], "To do");
    }

    #[test]
    fn empty_tags_roundtrip() {
        let blob = TransferBlob { tags: vec![], ..sample() };
        let recovered = TransferBlob::decode(&blob.encode().unwrap()).unwrap();
        assert!(recovered.tags.is_empty());
    }

    #[test]
    fn unicode_title_and_content_roundtrip() {
        let blob = TransferBlob {
            title: "日本語のノート 🔒".into(),
            content: json!({ "body": "Привет мир\nمرحبا" }),
            ..sample()
        };
        let recovered = TransferBlob::decode(&blob.encode().unwrap()).unwrap();
        assert_eq!(recovered.title, "日本語のノート 🔒");
    }
}
