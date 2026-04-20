mod db;

use chrono::Utc;
use db::Db;
use git2::{BranchType, Cred, CredentialType, FetchOptions, FetchPrune, PushOptions, RemoteCallbacks, Repository};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

fn relative_time(timestamp: i64) -> String {
    let diff = Utc::now().timestamp().saturating_sub(timestamp);
    match diff {
        d if d < 60 => "just now".to_string(),
        d if d < 3600 => {
            let m = d / 60;
            format!("{} minute{} ago", m, if m == 1 { "" } else { "s" })
        }
        d if d < 86400 => {
            let h = d / 3600;
            format!("{} hour{} ago", h, if h == 1 { "" } else { "s" })
        }
        d if d < 2_592_000 => {
            let days = d / 86400;
            format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
        }
        d if d < 31_536_000 => {
            let months = d / 2_592_000;
            format!("{} month{} ago", months, if months == 1 { "" } else { "s" })
        }
        d => {
            let years = d / 31_536_000;
            format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
        }
    }
}

fn make_remote_callbacks<'a>() -> RemoteCallbacks<'a> {
    let mut callbacks = RemoteCallbacks::new();
    let mut attempts = 0u8;
    callbacks.credentials(move |_url, username_from_url, allowed_types| {
        attempts += 1;
        if attempts > 3 {
            return Err(git2::Error::from_str("authentication failed"));
        }
        let username = username_from_url.unwrap_or("git");
        if allowed_types.contains(CredentialType::SSH_KEY) {
            if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                return Ok(cred);
            }
            let home = std::env::var("HOME").unwrap_or_default();
            for key_name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
                let private = std::path::Path::new(&home).join(".ssh").join(key_name);
                if private.exists() {
                    let public = private.with_extension("pub");
                    if let Ok(cred) = Cred::ssh_key(username, Some(&public), &private, None) {
                        return Ok(cred);
                    }
                }
            }
        }
        if allowed_types.contains(CredentialType::DEFAULT) {
            return Cred::default();
        }
        Err(git2::Error::from_str("no suitable credentials"))
    });
    callbacks
}

fn fetch_branches_from_git(path: &str, base_branch: &str) -> Result<Vec<BranchInfo>, String> {
    let repo = Repository::open(path).map_err(|e| e.message().to_string())?;

    let current_branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()))
        .unwrap_or_default();

    let base_oid = repo
        .revparse_single(base_branch)
        .ok()
        .and_then(|obj| obj.peel_to_commit().ok())
        .map(|c| c.id());

    let merged: HashSet<String> = if let Some(base_oid) = base_oid {
        repo.branches(Some(BranchType::Local))
            .map_err(|e| e.message().to_string())?
            .filter_map(|b| b.ok())
            .filter_map(|(branch, _)| {
                let name = branch.name().ok().flatten()?.to_string();
                let tip = branch.get().peel_to_commit().ok()?.id();
                let is_merged =
                    base_oid == tip || repo.graph_descendant_of(base_oid, tip).unwrap_or(false);
                if is_merged { Some(name) } else { None }
            })
            .collect()
    } else {
        HashSet::new()
    };

    let branches = repo
        .branches(Some(BranchType::Local))
        .map_err(|e| e.message().to_string())?
        .filter_map(|b| b.ok())
        .filter_map(|(branch, _)| {
            let name = branch.name().ok().flatten()?.to_string();
            let commit = branch.get().peel_to_commit().ok()?;
            let hash = commit.id().to_string()[..7].to_string();
            let msg = commit.summary().unwrap_or("").to_string();
            let date = relative_time(commit.time().seconds());
            let upstream = branch
                .upstream()
                .ok()
                .and_then(|u| u.name().ok().flatten().map(|s| s.to_string()));
            let is_current = name == current_branch;
            let is_merged = merged.contains(&name) && !is_current;
            Some(BranchInfo {
                name,
                is_current,
                last_commit_hash: hash,
                last_commit_message: msg,
                last_commit_date: date,
                is_merged,
                upstream,
            })
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

    let repo = Repository::open(&path).map_err(|e| e.message().to_string())?;
    let remote_names: Vec<String> = repo
        .remotes()
        .map_err(|e| e.message().to_string())?
        .iter()
        .flatten()
        .map(|s| s.to_string())
        .collect();

    for remote_name in &remote_names {
        let mut remote = repo
            .find_remote(remote_name)
            .map_err(|e| e.message().to_string())?;
        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(make_remote_callbacks());
        fetch_opts.prune(FetchPrune::On);
        remote
            .fetch(&[] as &[&str], Some(&mut fetch_opts), None)
            .map_err(|e| e.message().to_string())?;
    }

    // Refresh cache after fetch
    if let Ok(branches) = fetch_branches_from_git(&path, "HEAD") {
        let conn = db.0.lock().expect("db lock poisoned");
        update_branch_cache(&conn, &project_id, &branches);
    }

    Ok(format!("Fetched {} remote(s)", remote_names.len()))
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

    let repo = Repository::open(&project.path).map_err(|e| e.message().to_string())?;

    for req in &branches {
        if req.delete_remote {
            match repo.find_remote("origin") {
                Ok(mut remote) => {
                    let refspec = format!(":refs/heads/{}", req.branch_name);
                    let mut push_opts = PushOptions::new();
                    push_opts.remote_callbacks(make_remote_callbacks());
                    if let Err(e) = remote.push(&[refspec.as_str()], Some(&mut push_opts)) {
                        failed.push(DeleteFailure {
                            branch: format!("{} (remote)", req.branch_name),
                            error: e.message().to_string(),
                            needs_force: false,
                        });
                    }
                }
                Err(e) => {
                    failed.push(DeleteFailure {
                        branch: format!("{} (remote)", req.branch_name),
                        error: e.message().to_string(),
                        needs_force: false,
                    });
                }
            }
        }

        match repo.find_branch(&req.branch_name, BranchType::Local) {
            Ok(mut branch) => {
                if !force {
                    let head_oid = repo.head().and_then(|h| h.peel_to_commit()).map(|c| c.id());
                    let tip_oid = branch.get().peel_to_commit().map(|c| c.id());
                    if let (Ok(head), Ok(tip)) = (head_oid, tip_oid) {
                        let is_merged =
                            head == tip || repo.graph_descendant_of(head, tip).unwrap_or(false);
                        if !is_merged {
                            failed.push(DeleteFailure {
                                branch: req.branch_name.clone(),
                                error: format!(
                                    "The branch '{}' is not fully merged.",
                                    req.branch_name
                                ),
                                needs_force: true,
                            });
                            continue;
                        }
                    }
                }
                match branch.delete() {
                    Ok(()) => deleted.push(req.branch_name.clone()),
                    Err(e) => failed.push(DeleteFailure {
                        branch: req.branch_name.clone(),
                        error: e.message().to_string(),
                        needs_force: false,
                    }),
                }
            }
            Err(e) => {
                failed.push(DeleteFailure {
                    branch: req.branch_name.clone(),
                    error: e.message().to_string(),
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
    // Phase 1: read all DB data while holding the lock, then release it.
    let (projects, total_deleted, cached_summaries, uncached_ids) = {
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

        let mut cached_summaries: Vec<ProjectSummary> = Vec::new();
        let mut uncached_ids: Vec<String> = Vec::new();

        for p in &projects {
            let cached_count: usize = conn
                .query_row(
                    "SELECT COUNT(*) FROM cached_branches WHERE project_id = ?1",
                    [&p.id],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            if cached_count > 0 {
                let current_branch: String = conn
                    .query_row(
                        "SELECT name FROM cached_branches WHERE project_id = ?1 AND is_current = 1",
                        [&p.id],
                        |row| row.get(0),
                    )
                    .unwrap_or_default();

                cached_summaries.push(ProjectSummary {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    path: p.path.clone(),
                    branch_count: cached_count,
                    current_branch,
                });
            } else {
                uncached_ids.push(p.id.clone());
            }
        }

        (projects, total_deleted, cached_summaries, uncached_ids)
        // lock is released here
    };

    // Phase 2: for uncached projects, run git outside the lock and populate cache.
    let mut fresh_summaries: Vec<ProjectSummary> = Vec::new();
    for p in projects.iter().filter(|p| uncached_ids.contains(&p.id)) {
        let branches = fetch_branches_from_git(&p.path, "HEAD").unwrap_or_default();
        let branch_count = branches.len();
        let current_branch = branches
            .iter()
            .find(|b| b.is_current)
            .map(|b| b.name.clone())
            .unwrap_or_default();

        // Persist to cache so future Dashboard loads are instant.
        let conn = db.0.lock().expect("db lock poisoned");
        update_branch_cache(&conn, &p.id, &branches);
        drop(conn);

        fresh_summaries.push(ProjectSummary {
            id: p.id.clone(),
            name: p.name.clone(),
            path: p.path.clone(),
            branch_count,
            current_branch,
        });
    }

    // Phase 3: merge results preserving original project order.
    let mut projects_summary: Vec<ProjectSummary> = Vec::new();
    let mut total_branches = 0usize;
    for p in &projects {
        if let Some(s) = cached_summaries.iter().find(|s| s.id == p.id) {
            total_branches += s.branch_count;
            projects_summary.push(ProjectSummary {
                id: s.id.clone(),
                name: s.name.clone(),
                path: s.path.clone(),
                branch_count: s.branch_count,
                current_branch: s.current_branch.clone(),
            });
        } else if let Some(s) = fresh_summaries.iter().find(|s| s.id == p.id) {
            total_branches += s.branch_count;
            projects_summary.push(ProjectSummary {
                id: s.id.clone(),
                name: s.name.clone(),
                path: s.path.clone(),
                branch_count: s.branch_count,
                current_branch: s.current_branch.clone(),
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
