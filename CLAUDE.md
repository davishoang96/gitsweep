# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

GitSweep is a cross-platform desktop application built with **Tauri 2 + React 18 + TypeScript**. The Rust backend handles system-level operations (git commands, file system) while the React frontend provides the UI. Communication happens via Tauri's IPC `invoke()` bridge.

The app lets users add local git repositories, inspect their branches, detect merged branches, fetch from remotes, and bulk-delete stale branches — with a full deletion history log.

## Development Commands

```bash
# Start development (launches Vite dev server + Tauri window)
npm run tauri dev

# Production build
npm run tauri build

# Frontend only (no Tauri window, browser at localhost:1420)
npm run dev

# Type-check + bundle frontend
npm run build
```

## Architecture

### Frontend → Backend Communication

React calls Rust functions via `invoke()`:
```ts
import { invoke } from "@tauri-apps/api/core";
const result = await invoke<string>("command_name", { param: value });
```

Rust exposes commands with `#[tauri::command]` and registers them in `src-tauri/src/lib.rs` inside `.invoke_handler(tauri::generate_handler![...])`.

### Key Directories

- `src/` — React/TypeScript UI
- `src/pages/` — Page components (Dashboard, Projects, ProjectDetail, History)
- `src/types.ts` — Shared TypeScript interfaces (must stay in sync with Rust structs)
- `src-tauri/src/lib.rs` — All Tauri command handlers and data models (IPC entry points)
- `src-tauri/src/main.rs` — Binary entry point (minimal, just calls lib)
- `src-tauri/capabilities/default.json` — Security permissions (add new plugin permissions here)
- `src-tauri/tauri.conf.json` — App config: window size, bundle targets, dev URL

### Persistence

App data (projects list, deletion history) is stored as JSON at the OS app-data directory via `tauri::AppHandle::path().app_data_dir()`. Loaded/saved with `load_data` / `save_data` helpers in `lib.rs`.

### Tauri Commands

| Command | Description |
|---|---|
| `add_project` | Register a local git repo by name + path |
| `get_projects` | List all registered projects |
| `remove_project` | Remove a project by id (does not delete the repo) |
| `get_branches` | List branches for a project; accepts optional `base_branch` for merge detection |
| `delete_branches` | Bulk-delete branches (local and optionally remote); supports force flag |
| `fetch_project` | Run `git fetch --prune --all` on a project |
| `get_dashboard_stats` | Aggregate stats + per-project summary for the Dashboard |
| `get_deleted_branches` | Return full deletion history |
| `clear_history` | Wipe the deletion history |

### Adding a New Tauri Command

1. Define the function in `src-tauri/src/lib.rs` with `#[tauri::command]`
2. Register it in `generate_handler![...]`
3. Add any new types to both `lib.rs` (Rust struct) and `src/types.ts` (TS interface) — field names must match (serde uses snake_case by default)
4. Call it from the frontend with `invoke("function_name", { args })`

### Tauri Plugins

Active plugins: `tauri-plugin-opener`, `tauri-plugin-dialog`.

To add a plugin: `cargo add tauri-plugin-<name>` in `src-tauri/`, register it in `lib.rs` with `.plugin(tauri_plugin_name::init())`, and add the permission to `capabilities/default.json`.
