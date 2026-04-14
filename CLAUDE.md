# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

GitSweep is a cross-platform desktop application built with **Tauri 2 + React 18 + TypeScript**. The Rust backend handles system-level operations (git commands, file system) while the React frontend provides the UI. Communication happens via Tauri's IPC `invoke()` bridge.

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
- `src-tauri/src/lib.rs` — Tauri command handlers (IPC entry points)
- `src-tauri/src/main.rs` — Binary entry point (minimal, just calls lib)
- `src-tauri/capabilities/default.json` — Security permissions (add new plugin permissions here)
- `src-tauri/tauri.conf.json` — App config: window size, bundle targets, dev URL

### Adding a New Tauri Command

1. Define the function in `src-tauri/src/lib.rs` with `#[tauri::command]`
2. Register it in `generate_handler![...]`
3. Call it from the frontend with `invoke("function_name", { args })`

### Tauri Plugins

To add a plugin: `cargo add tauri-plugin-<name>` in `src-tauri/`, register it in `lib.rs` with `.plugin(tauri_plugin_name::init())`, and add the permission to `capabilities/default.json`.
