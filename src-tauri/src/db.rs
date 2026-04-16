use rusqlite::Connection;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

pub fn init(app_data_dir: &std::path::Path) -> Db {
    std::fs::create_dir_all(app_data_dir).ok();
    let db_path = app_data_dir.join("gitsweep.db");
    let conn = Connection::open(&db_path).expect("failed to open database");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .expect("failed to set pragmas");
    run_migrations(&conn);
    Db(Mutex::new(conn))
}

fn run_migrations(conn: &Connection) {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS projects (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            path        TEXT NOT NULL UNIQUE,
            created_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS deleted_branches (
            id            TEXT PRIMARY KEY,
            project_id    TEXT NOT NULL,
            project_name  TEXT NOT NULL,
            branch_name   TEXT NOT NULL,
            deleted_at    TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cached_branches (
            project_id          TEXT NOT NULL,
            name                TEXT NOT NULL,
            is_current          INTEGER NOT NULL DEFAULT 0,
            last_commit_hash    TEXT NOT NULL DEFAULT '',
            last_commit_message TEXT NOT NULL DEFAULT '',
            last_commit_date    TEXT NOT NULL DEFAULT '',
            is_merged           INTEGER NOT NULL DEFAULT 0,
            upstream            TEXT,
            cached_at           TEXT NOT NULL,
            PRIMARY KEY (project_id, name)
        );

        CREATE INDEX IF NOT EXISTS idx_cached_branches_project
            ON cached_branches(project_id);

        CREATE INDEX IF NOT EXISTS idx_deleted_branches_project
            ON deleted_branches(project_id);
        ",
    )
    .expect("failed to run migrations");
}
