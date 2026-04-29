//! Agent Console — multi-file coding agent procedures.
//!
//! Provides project indexing and task tracking via PluresDB, enabling an AI
//! model to understand project structure and record multi-file coding tasks.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use walkdir::WalkDir;

use crate::plugins::executor::PluginCrudExecutor;
use crate::plugins::error::PluginError;

const PLUGIN_NAME: &str = "agent-console";

/// Directories to skip during indexing.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".next",
    "__pycache__",
    ".venv",
    "dist",
    "build",
    ".cache",
    "vendor",
];

/// Summary of an indexed project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub path: String,
    pub file_count: usize,
    pub files: Vec<IndexedFile>,
    pub languages: HashMap<String, usize>,
    pub total_size: u64,
}

/// A single indexed file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    pub path: String,
    pub relative_path: String,
    pub language: String,
    pub size: u64,
}

/// Result of a coding task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub description: String,
    pub status: String,
    pub files_modified: Vec<String>,
    pub test_result: Option<String>,
}

/// Infer language from file extension.
fn language_from_ext(ext: &str) -> &str {
    match ext {
        "rs" => "Rust",
        "ts" | "tsx" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "py" => "Python",
        "go" => "Go",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "swift" => "Swift",
        "kt" | "kts" => "Kotlin",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "json" => "JSON",
        "md" => "Markdown",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" => "CSS",
        "sql" => "SQL",
        "sh" | "bash" | "zsh" => "Shell",
        "dockerfile" => "Docker",
        "svelte" => "Svelte",
        _ => "Other",
    }
}

/// Index a project directory, returning a [`ProjectIndex`] and storing
/// file_index entities in PluresDB via the executor.
pub fn index_project(
    project_path: &str,
    executor: &PluginCrudExecutor,
) -> Result<ProjectIndex, PluginError> {
    let root = Path::new(project_path);
    if !root.is_dir() {
        return Err(PluginError::InvalidManifest(format!(
            "Project path does not exist or is not a directory: {project_path}"
        )));
    }

    let mut files = Vec::new();
    let mut languages: HashMap<String, usize> = HashMap::new();
    let mut total_size: u64 = 0;

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !SKIP_DIRS.contains(&name.as_ref())
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let abs = entry.path().to_string_lossy().to_string();
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();
        let ext = entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let lang = language_from_ext(&ext).to_string();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        *languages.entry(lang.clone()).or_insert(0) += 1;
        total_size += size;

        files.push(IndexedFile {
            path: abs,
            relative_path: rel,
            language: lang,
            size,
        });
    }

    // Store each file as a file_index entity in PluresDB
    for f in &files {
        let _ = executor.create(
            "file_index",
            PLUGIN_NAME,
            json!({
                "path": f.relative_path,
                "language": f.language,
                "size_bytes": f.size,
            }),
        );
    }

    let file_count = files.len();

    // Create/update the project entity
    let now = chrono::Utc::now().to_rfc3339();
    let _ = executor.create(
        "project",
        PLUGIN_NAME,
        json!({
            "name": root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            "path": project_path,
            "indexed_at": now,
            "file_count": file_count,
        }),
    );

    Ok(ProjectIndex {
        path: project_path.to_string(),
        file_count,
        files,
        languages,
        total_size,
    })
}

/// Build a prompt-ready description of project structure for task planning.
pub fn plan_changes(task: &str, index: &ProjectIndex) -> String {
    let mut lang_summary: Vec<String> = index
        .languages
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    lang_summary.sort();

    let top_files: Vec<&str> = index
        .files
        .iter()
        .take(50)
        .map(|f| f.relative_path.as_str())
        .collect();

    format!(
        "Project: {} ({} files, {:.1} KB total)\n\
         Languages: {}\n\
         Task: {task}\n\
         Files (first 50):\n{}",
        index.path,
        index.file_count,
        index.total_size as f64 / 1024.0,
        lang_summary.join(", "),
        top_files
            .iter()
            .map(|f| format!("  - {f}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Record a coding task in PluresDB. Returns the task entity ID.
pub fn record_task(
    description: &str,
    status: &str,
    files_modified: &[String],
    test_result: Option<&str>,
    executor: &PluginCrudExecutor,
) -> Result<String, PluginError> {
    executor.create(
        "task",
        PLUGIN_NAME,
        json!({
            "description": description,
            "status": status,
            "files_modified": files_modified.len(),
            "test_result": test_result.unwrap_or("not_run"),
        }),
    )
}

/// Tool-facing: execute project indexing and return a JSON summary.
pub fn tool_project_index(
    path: &str,
    executor: &PluginCrudExecutor,
) -> Result<Value, PluginError> {
    let index = index_project(path, executor)?;
    Ok(json!({
        "path": index.path,
        "file_count": index.file_count,
        "total_size_bytes": index.total_size,
        "languages": index.languages,
        "files": index.files.iter().take(100).map(|f| json!({
            "path": f.relative_path,
            "language": f.language,
            "size": f.size,
        })).collect::<Vec<_>>(),
    }))
}

/// Tool-facing: create a coding task record and return its context.
pub fn tool_code_task(
    task: &str,
    project_path: &str,
    executor: &PluginCrudExecutor,
) -> Result<Value, PluginError> {
    // Index the project first (idempotent — will add new file_index entities)
    let index = index_project(project_path, executor)?;
    let plan = plan_changes(task, &index);

    // Record the task as pending
    let task_id = record_task(task, "pending", &[], None, executor)?;

    Ok(json!({
        "task_id": task_id,
        "project_context": plan,
        "file_count": index.file_count,
        "languages": index.languages,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_detection() {
        assert_eq!(language_from_ext("rs"), "Rust");
        assert_eq!(language_from_ext("ts"), "TypeScript");
        assert_eq!(language_from_ext("py"), "Python");
        assert_eq!(language_from_ext("xyz"), "Other");
    }

    #[test]
    fn plan_changes_format() {
        let index = ProjectIndex {
            path: "/tmp/test".into(),
            file_count: 2,
            files: vec![
                IndexedFile {
                    path: "/tmp/test/main.rs".into(),
                    relative_path: "main.rs".into(),
                    language: "Rust".into(),
                    size: 1024,
                },
                IndexedFile {
                    path: "/tmp/test/lib.rs".into(),
                    relative_path: "lib.rs".into(),
                    language: "Rust".into(),
                    size: 512,
                },
            ],
            languages: [("Rust".into(), 2)].into_iter().collect(),
            total_size: 1536,
        };
        let result = plan_changes("fix bugs", &index);
        assert!(result.contains("fix bugs"));
        assert!(result.contains("main.rs"));
        assert!(result.contains("Rust: 2"));
    }
}
