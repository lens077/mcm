//! The subset of the XMind content model we emit
//! (research-xmind.md §2; official xmind-generator serializer as the template).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sheet {
    pub id: String,
    pub class: String,
    pub title: String,
    #[serde(rename = "rootTopic")]
    pub root_topic: Topic,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub id: String,
    pub class: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Notes>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Children>,
}

impl Topic {
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            class: "topic".to_owned(),
            title: title.into(),
            notes: None,
            labels: Vec::new(),
            markers: Vec::new(),
            children: None,
        }
    }

    /// Attaches children, omitting the field entirely when there are none.
    pub fn attach(&mut self, children: Vec<Topic>) {
        if children.is_empty() {
            self.children = None;
        } else {
            self.children = Some(Children { attached: children });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Children {
    pub attached: Vec<Topic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notes {
    pub plain: PlainNote,
}

impl Notes {
    /// XMind's own generator always terminates plain notes with a newline.
    #[must_use]
    pub fn plain(content: &str) -> Self {
        let mut text = content.to_owned();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        Self {
            plain: PlainNote { content: text },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlainNote {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    #[serde(rename = "markerId")]
    pub marker_id: String,
}

impl Marker {
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self {
            marker_id: id.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub class: String,
    #[serde(rename = "end1Id")]
    pub end1_id: String,
    #[serde(rename = "end2Id")]
    pub end2_id: String,
    pub title: String,
}

impl Relationship {
    #[must_use]
    pub fn new(id: impl Into<String>, from: &str, to: &str, title: &str) -> Self {
        Self {
            id: id.into(),
            class: "relationship".to_owned(),
            end1_id: from.to_owned(),
            end2_id: to.to_owned(),
            title: title.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub creator: Creator,
    #[serde(rename = "dataStructureVersion")]
    pub data_structure_version: String,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            creator: Creator {
                name: "MCM".to_owned(),
            },
            data_structure_version: "2".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Creator {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "file-entries")]
    pub file_entries: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Manifest {
    /// Lists every payload entry, as XMind requires.
    #[must_use]
    pub fn for_entries(names: &[&str]) -> Self {
        let mut entries = std::collections::BTreeMap::new();
        for name in names {
            entries.insert((*name).to_owned(), serde_json::json!({}));
        }
        Self {
            file_entries: entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_omit_empty_children() {
        let mut topic = Topic::new("id", "标题");
        topic.attach(vec![]);
        let json = serde_json::to_string(&topic).expect("serialize");
        assert!(!json.contains("children"), "{json}");
    }

    #[test]
    fn topics_serialize_attached_children() {
        let mut topic = Topic::new("root", "根");
        topic.attach(vec![Topic::new("child", "子")]);
        let json = serde_json::to_string(&topic).expect("serialize");
        assert!(json.contains("\"children\":{\"attached\""), "{json}");
    }

    #[test]
    fn plain_notes_always_end_with_a_newline() {
        assert_eq!(Notes::plain("备注").plain.content, "备注\n");
        assert_eq!(Notes::plain("已有换行\n").plain.content, "已有换行\n");
    }

    #[test]
    fn relationships_use_xmind_field_names() {
        let json =
            serde_json::to_string(&Relationship::new("r1", "a", "b", "依赖")).expect("serialize");
        assert!(json.contains("\"end1Id\":\"a\""), "{json}");
        assert!(json.contains("\"end2Id\":\"b\""), "{json}");
        assert!(json.contains("\"class\":\"relationship\""), "{json}");
    }

    #[test]
    fn metadata_declares_data_structure_version_two() {
        let json = serde_json::to_string(&Metadata::default()).expect("serialize");
        assert!(json.contains("\"dataStructureVersion\":\"2\""), "{json}");
        assert!(json.contains("\"name\":\"MCM\""), "{json}");
    }

    #[test]
    fn manifest_lists_every_entry() {
        let manifest = Manifest::for_entries(&["content.json", "metadata.json"]);
        assert_eq!(manifest.file_entries.len(), 2);
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains("\"file-entries\""), "{json}");
    }
}
