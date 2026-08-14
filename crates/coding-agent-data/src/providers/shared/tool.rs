use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{FileChange, FileChangeKind, ToolKind, ToolLocation, ToolSourceKind, ToolStatus};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ObservedTool {
    pub name: String,
    pub namespace: Option<String>,
    pub source_kind: ToolSourceKind,
    pub server_name: Option<String>,
    pub kind: ToolKind,
    pub input: Value,
    pub locations: Vec<ToolLocation>,
}

impl ObservedTool {
    pub fn new(name: &str, namespace: Option<&str>, input: &Value) -> Self {
        let (name, namespace) = identity(name, namespace);
        let kind = kind(&name, namespace.as_deref());
        Self {
            locations: locations(input, kind),
            kind,
            input: input.clone(),
            name,
            namespace,
            source_kind: ToolSourceKind::Unknown,
            server_name: None,
        }
    }

    pub fn with_source(mut self, source_kind: ToolSourceKind, server_name: Option<&str>) -> Self {
        self.source_kind = source_kind;
        self.server_name = server_name
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        self
    }
}

pub(crate) fn identity(name: &str, namespace: Option<&str>) -> (String, Option<String>) {
    if let Some(namespace) = namespace.filter(|value| !value.is_empty()) {
        return (name.to_owned(), Some(namespace.to_owned()));
    }

    let mut components = name.splitn(3, "__");
    if components.next() == Some("mcp") {
        if let (Some(namespace), Some(name)) = (components.next(), components.next()) {
            if !namespace.is_empty() && !name.is_empty() {
                return (name.to_owned(), Some(namespace.to_owned()));
            }
        }
    }

    (name.to_owned(), None)
}

pub(crate) fn kind(name: &str, namespace: Option<&str>) -> ToolKind {
    let qualified = namespace
        .filter(|value| !value.is_empty())
        .map(|namespace| format!("{namespace}.{name}"))
        .unwrap_or_else(|| name.to_owned());
    kind_from_name(&qualified)
}

fn kind_from_name(name: &str) -> ToolKind {
    let name = name.to_ascii_lowercase();
    if name.contains("read") || name.contains("view") || name.contains("open") {
        ToolKind::Read
    } else if name.contains("edit")
        || name.contains("write")
        || name.contains("patch")
        || name.contains("create")
    {
        ToolKind::Edit
    } else if name.contains("delete") || name.contains("remove") {
        ToolKind::Delete
    } else if name.contains("move") || name.contains("rename") {
        ToolKind::Move
    } else if name.contains("search")
        || name.contains("find")
        || matches!(name.as_str(), "rg" | "grep" | "glob")
    {
        ToolKind::Search
    } else if name.contains("exec")
        || name.contains("shell")
        || name.contains("command")
        || name.contains("terminal")
        || matches!(name.as_str(), "bash" | "powershell" | "python" | "node")
    {
        ToolKind::Execute
    } else if name.contains("think") || name.contains("reason") || name.contains("plan") {
        ToolKind::Think
    } else if name.contains("switch_mode")
        || name.contains("switchmode")
        || name.contains("mode_switch")
    {
        ToolKind::SwitchMode
    } else if name.contains("fetch")
        || name.contains("http")
        || name.contains("browser")
        || name.contains("web")
    {
        ToolKind::Fetch
    } else {
        ToolKind::Other
    }
}

pub(crate) fn file_changes(
    tool: &ObservedTool,
    output: &Value,
    status: ToolStatus,
) -> Vec<FileChange> {
    let Some(kind) = file_change_kind(tool, output) else {
        return Vec::new();
    };
    let diff = ["diff", "patch", "structuredPatch", "structured_patch"]
        .into_iter()
        .find_map(|key| output.get(key).or_else(|| tool.input.get(key)))
        .and_then(nonempty_json_text);

    if kind == FileChangeKind::Move {
        let new_path = path_field(
            [&tool.input, output],
            &[
                "new_path",
                "newPath",
                "move_path",
                "movePath",
                "destination_path",
                "destinationPath",
            ],
        );
        let old_path = path_field(
            [&tool.input, output],
            &[
                "old_path",
                "oldPath",
                "source_path",
                "sourcePath",
                "file_path",
                "filePath",
                "path",
            ],
        );
        return new_path
            .map(|path| FileChange {
                path,
                old_path,
                kind,
                diff,
                status,
            })
            .into_iter()
            .collect();
    }

    let mut paths = tool
        .locations
        .iter()
        .map(|location| (location.path.clone(), location.line))
        .collect::<BTreeSet<_>>();
    paths.extend(
        locations(output, tool.kind)
            .into_iter()
            .map(|location| (location.path, location.line)),
    );
    paths
        .into_iter()
        .map(|(path, _)| FileChange {
            path,
            old_path: None,
            kind,
            diff: diff.clone(),
            status,
        })
        .collect()
}

fn file_change_kind(tool: &ObservedTool, output: &Value) -> Option<FileChangeKind> {
    let name = tool.name.to_ascii_lowercase();
    if name.contains("delete") || name.contains("remove") {
        Some(FileChangeKind::Delete)
    } else if name.contains("move") || name.contains("rename") {
        Some(FileChangeKind::Move)
    } else if name.contains("create") {
        Some(FileChangeKind::Create)
    } else if name.contains("write") {
        match output
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("create" | "created" | "add" | "added") => Some(FileChangeKind::Create),
            Some("update" | "updated" | "modify" | "modified") => Some(FileChangeKind::Update),
            _ => Some(FileChangeKind::Write),
        }
    } else if name.contains("edit") || name.contains("patch") {
        Some(FileChangeKind::Update)
    } else {
        None
    }
}

fn locations(input: &Value, kind: ToolKind) -> Vec<ToolLocation> {
    let mut locations = BTreeSet::new();
    collect_locations(input, kind, &mut locations);
    locations
        .into_iter()
        .map(|(path, line)| ToolLocation { path, line })
        .collect()
}

fn collect_locations(
    value: &Value,
    kind: ToolKind,
    locations: &mut BTreeSet<(PathBuf, Option<u64>)>,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_locations(value, kind, locations);
            }
        }
        Value::Object(fields) => {
            let line = [
                "line",
                "line_number",
                "lineNumber",
                "start_line",
                "startLine",
            ]
            .into_iter()
            .find_map(|key| fields.get(key).and_then(Value::as_u64));

            for key in [
                "file_path",
                "filePath",
                "notebook_path",
                "notebookPath",
                "target_path",
                "targetPath",
                "old_path",
                "oldPath",
                "new_path",
                "newPath",
                "move_path",
                "movePath",
                "saved_path",
                "savedPath",
            ] {
                if let Some(path) = fields.get(key).and_then(Value::as_str) {
                    insert_path(path, line, locations);
                }
            }

            if is_path_oriented(kind) {
                if let Some(path) = fields.get("path").and_then(Value::as_str) {
                    insert_path(path, line, locations);
                }
                for key in ["paths", "files"] {
                    if let Some(values) = fields.get(key).and_then(Value::as_array) {
                        for value in values {
                            if let Some(path) = value.as_str() {
                                insert_path(path, None, locations);
                            } else {
                                collect_locations(value, kind, locations);
                            }
                        }
                    }
                }
            }

            if let Some(changes) = fields.get("changes").and_then(Value::as_object) {
                for path in changes.keys() {
                    insert_path(path, None, locations);
                }
            }

            for value in fields.values() {
                if value.is_array() || value.is_object() {
                    collect_locations(value, kind, locations);
                }
            }
        }
        _ => {}
    }
}

fn insert_path(value: &str, line: Option<u64>, locations: &mut BTreeSet<(PathBuf, Option<u64>)>) {
    let value = value.trim();
    if value.is_empty()
        || value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("data:")
    {
        return;
    }
    locations.insert((PathBuf::from(value), line));
}

fn is_path_oriented(kind: ToolKind) -> bool {
    matches!(
        kind,
        ToolKind::Read | ToolKind::Edit | ToolKind::Delete | ToolKind::Move | ToolKind::Search
    )
}

fn path_field<'a>(values: impl IntoIterator<Item = &'a Value>, keys: &[&str]) -> Option<PathBuf> {
    for value in values {
        for key in keys {
            if let Some(path) = value
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

fn nonempty_json_text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| serde_json::to_string(value).ok())
        .filter(|value| !value.is_empty() && value != "null")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn splits_mcp_tool_identity() {
        assert_eq!(
            identity("mcp__github__search_code", None),
            ("search_code".to_owned(), Some("github".to_owned()))
        );
    }

    #[test]
    fn extracts_only_explicit_file_locations() {
        let locations = locations(
            &json!({
                "cwd": "/ignored",
                "file_path": "/tmp/a.rs",
                "line": 7,
                "changes": {
                    "src/lib.rs": { "type": "update" }
                },
                "nested": { "newPath": "src/new.rs" }
            }),
            ToolKind::Edit,
        );

        assert_eq!(
            locations,
            vec![
                ToolLocation {
                    path: PathBuf::from("/tmp/a.rs"),
                    line: Some(7),
                },
                ToolLocation {
                    path: PathBuf::from("src/lib.rs"),
                    line: None,
                },
                ToolLocation {
                    path: PathBuf::from("src/new.rs"),
                    line: None,
                },
            ]
        );
    }

    #[test]
    fn generic_paths_require_a_path_oriented_tool() {
        assert!(locations(&json!({ "path": "selector.value" }), ToolKind::Other).is_empty());
        assert_eq!(
            locations(&json!({ "path": "src/lib.rs" }), ToolKind::Read),
            vec![ToolLocation {
                path: PathBuf::from("src/lib.rs"),
                line: None,
            }]
        );
    }

    #[test]
    fn move_changes_are_not_duplicated_by_old_and_new_locations() {
        let tool = ObservedTool::new(
            "rename_file",
            None,
            &json!({ "old_path": "old.rs", "new_path": "new.rs" }),
        );
        assert_eq!(
            file_changes(&tool, &Value::Null, ToolStatus::Completed),
            vec![FileChange {
                path: PathBuf::from("new.rs"),
                old_path: Some(PathBuf::from("old.rs")),
                kind: FileChangeKind::Move,
                diff: None,
                status: ToolStatus::Completed,
            }]
        );
    }

    #[test]
    fn ambiguous_writes_do_not_claim_that_a_file_already_existed() {
        let tool = ObservedTool::new("Write", None, &json!({ "file_path": "new.rs" }));
        assert_eq!(
            file_changes(&tool, &Value::Null, ToolStatus::Completed)[0].kind,
            FileChangeKind::Write
        );
    }
}
