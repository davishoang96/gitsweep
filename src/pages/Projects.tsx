import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Link } from "react-router-dom";
import { Project } from "../types";

export default function Projects() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [showModal, setShowModal] = useState(false);
  const [loading, setLoading] = useState(true);
  const [fetchingId, setFetchingId] = useState<string | null>(null);
  const [fetchError, setFetchError] = useState("");

  const load = () =>
    invoke<Project[]>("get_projects")
      .then(setProjects)
      .finally(() => setLoading(false));

  useEffect(() => { load(); }, []);

  const handleRemove = async (id: string) => {
    await invoke("remove_project", { id });
    load();
  };

  const handleFetch = async (id: string) => {
    setFetchingId(id);
    setFetchError("");
    try {
      await invoke("fetch_project", { projectId: id });
    } catch (err) {
      setFetchError(String(err));
    } finally {
      setFetchingId(null);
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <div className="page-title">Projects</div>
          <div className="page-subtitle">Manage your git repositories</div>
        </div>
        <button className="btn btn-primary" onClick={() => setShowModal(true)}>
          + Add Project
        </button>
      </div>

      {fetchError && <div className="error-msg">{fetchError}</div>}

      {loading ? (
        <div className="loading"><div className="spinner" />Loading…</div>
      ) : projects.length === 0 ? (
        <div className="card">
          <div className="empty-state">
            <div className="empty-state-icon">◫</div>
            <div className="empty-state-title">No projects added</div>
            <div className="empty-state-desc">
              Add a local git repository to start cleaning up branches.
            </div>
            <button className="btn btn-primary" style={{ marginTop: 12 }} onClick={() => setShowModal(true)}>
              Add Project
            </button>
          </div>
        </div>
      ) : (
        <div className="card">
          <div className="projects-grid">
            {projects.map((p) => (
              <div className="project-row" key={p.id}>
                <div className="project-info">
                  <div className="project-name">{p.name}</div>
                  <div className="project-path">{p.path}</div>
                </div>
                <div style={{ display: "flex", gap: 8 }}>
                  <button
                    className="btn btn-ghost"
                    disabled={fetchingId === p.id}
                    onClick={() => handleFetch(p.id)}
                  >
                    {fetchingId === p.id ? "Fetching…" : "⬇ Fetch"}
                  </button>
                  <Link to={`/projects/${p.id}`} className="btn btn-ghost">
                    Manage Branches
                  </Link>
                  <button
                    className="btn btn-ghost"
                    style={{ color: "var(--red-hover)", borderColor: "rgba(218,54,51,0.3)" }}
                    onClick={() => {
                      if (confirm(`Remove "${p.name}" from GitSweep?\n(This does not delete the repository.)`))
                        handleRemove(p.id);
                    }}
                  >
                    Remove
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {showModal && (
        <AddProjectModal
          onClose={() => setShowModal(false)}
          onAdded={() => { setShowModal(false); load(); }}
        />
      )}
    </div>
  );
}

function AddProjectModal({ onClose, onAdded }: { onClose: () => void; onAdded: () => void }) {
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const browsePath = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === "string") {
      setPath(selected);
      if (!name) {
        // Auto-fill project name from folder name
        const parts = selected.replace(/\\/g, "/").split("/");
        setName(parts[parts.length - 1] || "");
      }
    }
  };

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !path.trim()) { setError("Name and path are required."); return; }
    setSaving(true);
    setError("");
    try {
      await invoke("add_project", { name: name.trim(), path: path.trim() });
      onAdded();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Add Project</div>
        {error && <div className="error-msg">{error}</div>}
        <form onSubmit={submit}>
          <div className="form-group">
            <label className="form-label">Project Name</label>
            <input
              className="form-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="my-app"
              autoFocus
            />
          </div>
          <div className="form-group">
            <label className="form-label">Repository Path</label>
            <div className="input-with-btn">
              <input
                className="form-input"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="/path/to/repo"
              />
              <button type="button" className="btn btn-ghost" onClick={browsePath}>
                Browse
              </button>
            </div>
            <div className="form-hint">Must be the root of a git repository (contains .git folder)</div>
          </div>
          <div className="modal-footer">
            <button type="button" className="btn btn-ghost" onClick={onClose}>Cancel</button>
            <button type="submit" className="btn btn-primary" disabled={saving}>
              {saving ? "Adding…" : "Add Project"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
