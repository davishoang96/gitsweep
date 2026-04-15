use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use tauri::Manager;
use uuid::Uuid;

// ─── Data models ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedBranchRecord {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub branch_name: String,
    pub deleted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppData {
    pub projects: Vec<Project>,
    pub deleted_branches: Vec<DeletedBranchRecord>,
}

#[derive(Debug, Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub last_commit_hash: String,
    pub last_commit_message: String,
    pub last_commit_date: String,
    pub is_merged: bool,
    pub upstream: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub branch_count: usize,
    pub current_branch: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_projects: usize,
    pub total_branches: usize,
    pub total_deleted: usize,
    pub projects_summary: Vec<ProjectSummary>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteBranchRequest {
    pub branch_name: String,
    pub delete_remote: bool,
}

#[derive(Debug, Serialize)]
pub struct DeleteFailure {
    pub branch: String,
    pub error: String,
    pub needs_force: bool,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub deleted: Vec<String>,
    pub failed: Vec<DeleteFailure>,
}

// ─── Persistence ─────────────────────────────────────────────────────────────

fn data_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("could not resolve app data dir");
    std::fs::create_dir_all(&dir).ok();
    dir.join("data.json")
}

fn load_data(app: &tauri::AppHandle) -> AppData {
    let path = data_path(app);
    if path.exists() {
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        AppData::default()
    }
}

fn save_data(app: &tauri::AppHandle, data: &AppData) -> Result<(), String> {
    let content = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(data_path(app), content).map_err(|e| e.to_string())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn git(args: &[&str], cwd: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn git_silent(args: &[&str], cwd: &str) -> String {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

// ─── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
fn add_project(app: tauri::AppHandle, name: String, path: String) -> Result<Project, String> {
    if !std::path::Path::new(&path).join(".git").exists() {
        return Err(format!(
            "'{}' is not a git repository (no .git directory found)",
            path
        ));
    }

    let mut data = load_data(&app);

    if data.projects.iter().any(|p| p.path == path) {
        return Err("A project with this path already exists".into());
    }

    let project = Project {
        id: Uuid::new_v4().to_string(),
        name,
        path,
        created_at: Utc::now().to_rfc3339(),
    };

    data.projects.push(project.clone());
    save_data(&app, &data)?;
    Ok(project)
}

#[tauri::command]
fn get_projects(app: tauri::AppHandle) -> Vec<Project> {
    load_data(&app).projects
}

#[tauri::command]
fn remove_project(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut data = load_data(&app);
    data.projects.retain(|p| p.id != id);
    save_data(&app, &data)
}

#[tauri::command]
fn fetch_project(app: tauri::AppHandle, project_id: String) -> Result<String, String> {
    let data = load_data(&app);
    let project = data
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    git(&["fetch", "--prune", "--all"], &project.path)
}

#[tauri::command]
fn get_branches(
    app: tauri::AppHandle,
    project_id: String,
    base_branch: Option<String>,
) -> Result<Vec<BranchInfo>, String> {
    let data = load_data(&app);
    let project = data
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?;

    let current_branch = git_silent(&["rev-parse", "--abbrev-ref", "HEAD"], &project.path);

    let merge_target = base_branch.as_deref().unwrap_or("HEAD");
    let merged_raw = git_silent(&["branch", "--merged", merge_target], &project.path);
    let merged: HashSet<String> = merged_raw
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim().to_string())
        .collect();

    // Tab-separated fields so commit messages with "|" don't break parsing
    let fmt = "%(refname:short)\t%(objectname:short)\t%(subject)\t%(committerdate:relative)\t%(upstream:short)";
    let raw = git(&["branch", &format!("--format={}", fmt)], &project.path)?;

    let branches = raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut p = line.splitn(5, '\t');
            let name = p.next().unwrap_or("").to_string();
            let hash = p.next().unwrap_or("").to_string();
            let msg = p.next().unwrap_or("").to_string();
            let date = p.next().unwrap_or("").to_string();
            let upstream = p
                .next()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let is_current = name == current_branch;
            let is_merged = merged.contains(&name) && !is_current;

            BranchInfo {
                name,
                is_current,
                last_commit_hash: hash,
                last_commit_message: msg,
                last_commit_date: date,
                is_merged,
                upstream,
            }
        })
        .collect();

    Ok(branches)
}

#[tauri::command]
fn delete_branches(
    app: tauri::AppHandle,
    project_id: String,
    branches: Vec<DeleteBranchRequest>,
    force: bool,
) -> Result<DeleteResult, String> {
    let mut data = load_data(&app);
    let project = data
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("Project not found")?
        .clone();

    let mut deleted: Vec<String> = Vec::new();
    let mut failed: Vec<DeleteFailure> = Vec::new();

    for req in &branches {
        // Optionally push --delete to the remote first
        if req.delete_remote {
            match Command::new("git")
                .args(["push", "origin", "--delete", &req.branch_name])
                .current_dir(&project.path)
                .output()
            {
                Ok(out) if !out.status.success() => {
                    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    failed.push(DeleteFailure {
                        branch: format!("{} (remote)", req.branch_name),
                        error: err,
                        needs_force: false,
                    });
                }
                Err(e) => {
                    failed.push(DeleteFailure {
                        branch: format!("{} (remote)", req.branch_name),
                        error: e.to_string(),
                        needs_force: false,
                    });
                }
                Ok(_) => {}
            }
        }

        // Delete local branch
        let flag = if force { "-D" } else { "-d" };
        match Command::new("git")
            .args(["branch", flag, &req.branch_name])
            .current_dir(&project.path)
            .output()
        {
            Ok(out) if out.status.success() => {
                deleted.push(req.branch_name.clone());
                data.deleted_branches.push(DeletedBranchRecord {
                    id: Uuid::new_v4().to_string(),
                    project_id: project.id.clone(),
                    project_name: project.name.clone(),
                    branch_name: req.branch_name.clone(),
                    deleted_at: Utc::now().to_rfc3339(),
                });
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let needs_force = err.contains("not fully merged");
                failed.push(DeleteFailure {
                    branch: req.branch_name.clone(),
                    error: err,
                    needs_force,
                });
            }
            Err(e) => {
                failed.push(DeleteFailure {
                    branch: req.branch_name.clone(),
                    error: e.to_string(),
                    needs_force: false,
                });
            }
        }
    }

    save_data(&app, &data)?;
    Ok(DeleteResult { deleted, failed })
}

#[tauri::command]
fn get_dashboard_stats(app: tauri::AppHandle) -> DashboardStats {
    let data = load_data(&app);
    let mut total_branches = 0usize;
    let mut projects_summary = Vec::new();

    for p in &data.projects {
        let branch_count = git_silent(&["branch"], &p.path)
            .lines()
            .filter(|l| !l.is_empty())
            .count();
        total_branches += branch_count;

        let current_branch = git_silent(&["rev-parse", "--abbrev-ref", "HEAD"], &p.path);

        projects_summary.push(ProjectSummary {
            id: p.id.clone(),
            name: p.name.clone(),
            path: p.path.clone(),
            branch_count,
            current_branch,
        });
    }

    DashboardStats {
        total_projects: data.projects.len(),
        total_branches,
        total_deleted: data.deleted_branches.len(),
        projects_summary,
    }
}

#[tauri::command]
fn get_deleted_branches(app: tauri::AppHandle) -> Vec<DeletedBranchRecord> {
    load_data(&app).deleted_branches
}

#[tauri::command]
fn clear_history(app: tauri::AppHandle) -> Result<(), String> {
    let mut data = load_data(&app);
    data.deleted_branches.clear();
    save_data(&app, &data)
}

// ─── Entry ───────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            add_project,
            get_projects,
            remove_project,
            get_branches,
            delete_branches,
            get_dashboard_stats,
            get_deleted_branches,
            fetch_project,
            clear_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
