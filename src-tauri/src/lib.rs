mod db;

use chrono::Utc;
use db::Db;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

// ─── Helpers ────────────────────────────────────────────────────────────────

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

/// Fetch branches from git and return the list.
fn fetch_branches_from_git(path: &str, base_branch: &str) -> Result<Vec<BranchInfo>, String> {
    let current_branch = git_silent(&["rev-parse", "--abbrev-ref", "HEAD"], path);

    let merged_raw = git_silent(&["branch", "--merged", base_branch], path);
    let merged: HashSet<String> = merged_raw
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim().to_string())
        .collect();

    let fmt = "%(refname:short)\t%(objectname:short)\t%(subject)\t%(committerdate:relative)\t%(upstream:short)";
    let raw = git(&["branch", &format!("--format={}", fmt)], path)?;

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

/// Write branch list to the cached_branches table (delete-and-reinsert).
fn update_branch_cache(
    conn: &rusqlite::Connection,
    project_id: &str,
    branches: &[BranchInfo],
) {
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction().unwrap();

    tx.execute(
        "DELETE FROM cached_branches WHERE project_id = ?1",
        [project_id],
    )
    .ok();

    for b in branches {
        tx.execute(
            "INSERT INTO cached_branches (project_id, name, is_current, last_commit_hash, last_commit_message, last_commit_date, is_merged, upstream, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                project_id,
                b.name,
                b.is_current as i32,
                b.last_commit_hash,
                b.last_commit_message,
                b.last_commit_date,
                b.is_merged as i32,
                b.upstream,
                now,
            ],
        )
        .ok();
    }

    tx.commit().ok();
}

/// Migrate existing data.json into SQLite (runs once on first launch after update).
fn migrate_json_if_needed(app: &tauri::AppHandle, db: &Db) {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("could not resolve app data dir");
    let json_path = data_dir.join("data.json");
    if !json_path.exists() {
        return;
    }

    let raw = std::fs::read_to_string(&json_path).unwrap_or_default();
    let data: AppData = serde_json::from_str(&raw).unwrap_or_default();

    let conn = db.0.lock().expect("db lock poisoned");
    let tx = conn.unchecked_transaction().unwrap();

    for p in &data.projects {
        tx.execute(
            "INSERT OR IGNORE INTO projects (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![p.id, p.name, p.path, p.created_at],
        )
        .ok();
    }
    for d in &data.deleted_branches {
        tx.execute(
            "INSERT OR IGNORE INTO deleted_branches (id, project_id, project_name, branch_name, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![d.id, d.project_id, d.project_name, d.branch_name, d.deleted_at],
        )
        .ok();
    }

    tx.commit().unwrap();
    drop(conn);

    std::fs::rename(&json_path, json_path.with_extension("json.bak")).ok();
}

// ─── Commands ───────────────────────────────────────────────────────────────

#[tauri::command]
fn add_project(db: tauri::State<'_, Db>, name: String, path: String) -> Result<Project, String> {
    if !std::path::Path::new(&path).join(".git").exists() {
        return Err(format!(
            "'{}' is not a git repository (no .git directory found)",
            path
        ));
    }

    let conn = db.0.lock().expect("db lock poisoned");

    // Check for duplicate path
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM projects WHERE path = ?1",
            [&path],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if exists {
        return Err("A project with this path already exists".into());
    }

    let project = Project {
        id: Uuid::new_v4().to_string(),
        name,
        path,
        created_at: Utc::now().to_rfc3339(),
    };

    conn.execute(
        "INSERT INTO projects (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![project.id, project.name, project.path, project.created_at],
    )
    .map_err(|e| e.to_string())?;

    Ok(project)
}

#[tauri::command]
fn get_projects(db: tauri::State<'_, Db>) -> Vec<Project> {
    let conn = db.0.lock().expect("db lock poisoned");
    let mut stmt = conn
        .prepare("SELECT id, name, path, created_at FROM projects ORDER BY created_at")
        .unwrap();

    stmt.query_map([], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            created_at: row.get(3)?,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

#[tauri::command]
fn remove_project(db: tauri::State<'_, Db>, id: String) -> Result<(), String> {
    let conn = db.0.lock().expect("db lock poisoned");
    conn.execute("DELETE FROM cached_branches WHERE project_id = ?1", [&id])
        .ok();
    conn.execute("DELETE FROM projects WHERE id = ?1", [&id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn fetch_project(db: tauri::State<'_, Db>, project_id: String) -> Result<String, String> {
    let conn = db.0.lock().expect("db lock poisoned");
    let path: String = conn
        .query_row(
            "SELECT path FROM projects WHERE id = ?1",
            [&project_id],
            |row| row.get(0),
        )
        .map_err(|_| "Project not found".to_string())?;
    drop(conn);

    let result = git(&["fetch", "--prune", "--all"], &path)?;

    // Refresh cache after fetch
    if let Ok(branches) = fetch_branches_from_git(&path, "HEAD") {
        let conn = db.0.lock().expect("db lock poisoned");
        update_branch_cache(&conn, &project_id, &branches);
    }

    Ok(result)
}

#[tauri::command]
fn get_branches(
    db: tauri::State<'_, Db>,
    project_id: String,
    base_branch: Option<String>,
) -> Result<Vec<BranchInfo>, String> {
    let conn = db.0.lock().expect("db lock poisoned");
    let path: String = conn
        .query_row(
            "SELECT path FROM projects WHERE id = ?1",
            [&project_id],
            |row| row.get(0),
        )
        .map_err(|_| "Project not found".to_string())?;
    drop(conn);

    let merge_target = base_branch.as_deref().unwrap_or("HEAD");
    let branches = fetch_branches_from_git(&path, merge_target)?;

    // Update cache
    let conn = db.0.lock().expect("db lock poisoned");
    update_branch_cache(&conn, &project_id, &branches);

    Ok(branches)
}

#[tauri::command]
fn delete_branches(
    db: tauri::State<'_, Db>,
    project_id: String,
    branches: Vec<DeleteBranchRequest>,
    force: bool,
) -> Result<DeleteResult, String> {
    let conn = db.0.lock().expect("db lock poisoned");
    let project: Project = conn
        .query_row(
            "SELECT id, name, path, created_at FROM projects WHERE id = ?1",
            [&project_id],
            |row| {
                Ok(Project {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
        .map_err(|_| "Project not found".to_string())?;
    drop(conn);

    let mut deleted: Vec<String> = Vec::new();
    let mut failed: Vec<DeleteFailure> = Vec::new();

    for req in &branches {
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

        let flag = if force { "-D" } else { "-d" };
        match Command::new("git")
            .args(["branch", flag, &req.branch_name])
            .current_dir(&project.path)
            .output()
        {
            Ok(out) if out.status.success() => {
                deleted.push(req.branch_name.clone());
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

    // Record deletions in DB and update cache
    let conn = db.0.lock().expect("db lock poisoned");
    for name in &deleted {
        conn.execute(
            "INSERT INTO deleted_branches (id, project_id, project_name, branch_name, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                project.id,
                project.name,
                name,
                Utc::now().to_rfc3339(),
            ],
        )
        .ok();

        conn.execute(
            "DELETE FROM cached_branches WHERE project_id = ?1 AND name = ?2",
            params![project.id, name],
        )
        .ok();
    }

    Ok(DeleteResult { deleted, failed })
}

#[tauri::command]
fn get_dashboard_stats(db: tauri::State<'_, Db>) -> DashboardStats {
    let conn = db.0.lock().expect("db lock poisoned");

    let projects: Vec<Project> = {
        let mut stmt = conn
            .prepare("SELECT id, name, path, created_at FROM projects ORDER BY created_at")
            .unwrap();
        stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    };

    let total_deleted: usize = conn
        .query_row("SELECT COUNT(*) FROM deleted_branches", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let mut total_branches = 0usize;
    let mut projects_summary = Vec::new();

    for p in &projects {
        // Try to read from cache first
        let cached_count: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM cached_branches WHERE project_id = ?1",
                [&p.id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if cached_count > 0 {
            // Use cached data
            let current_branch: String = conn
                .query_row(
                    "SELECT name FROM cached_branches WHERE project_id = ?1 AND is_current = 1",
                    [&p.id],
                    |row| row.get(0),
                )
                .unwrap_or_default();

            total_branches += cached_count;
            projects_summary.push(ProjectSummary {
                id: p.id.clone(),
                name: p.name.clone(),
                path: p.path.clone(),
                branch_count: cached_count,
                current_branch,
            });
        } else {
            // No cache yet — fall back to git (only happens on first visit before viewing project)
            let branch_count = git_silent(&["branch"], &p.path)
                .lines()
                .filter(|l| !l.is_empty())
                .count();
            total_branches += branch_count;

            let current_branch =
                git_silent(&["rev-parse", "--abbrev-ref", "HEAD"], &p.path);

            projects_summary.push(ProjectSummary {
                id: p.id.clone(),
                name: p.name.clone(),
                path: p.path.clone(),
                branch_count,
                current_branch,
            });
        }
    }

    DashboardStats {
        total_projects: projects.len(),
        total_branches,
        total_deleted,
        projects_summary,
    }
}

#[tauri::command]
fn get_deleted_branches(db: tauri::State<'_, Db>) -> Vec<DeletedBranchRecord> {
    let conn = db.0.lock().expect("db lock poisoned");
    let mut stmt = conn
        .prepare("SELECT id, project_id, project_name, branch_name, deleted_at FROM deleted_branches ORDER BY deleted_at DESC")
        .unwrap();

    stmt.query_map([], |row| {
        Ok(DeletedBranchRecord {
            id: row.get(0)?,
            project_id: row.get(1)?,
            project_name: row.get(2)?,
            branch_name: row.get(3)?,
            deleted_at: row.get(4)?,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

#[tauri::command]
fn clear_history(db: tauri::State<'_, Db>) -> Result<(), String> {
    let conn = db.0.lock().expect("db lock poisoned");
    conn.execute("DELETE FROM deleted_branches", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Entry ──────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not resolve app data dir");
            let database = db::init(&data_dir);
            migrate_json_if_needed(app.handle(), &database);
            app.manage(database);
            Ok(())
        })
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
