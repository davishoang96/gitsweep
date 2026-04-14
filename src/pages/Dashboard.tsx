import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Link } from "react-router-dom";
import { DashboardStats } from "../types";

export default function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<DashboardStats>("get_dashboard_stats")
      .then(setStats)
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div className="loading">
        <div className="spinner" />
        Loading…
      </div>
    );
  }

  return (
    <div>
      <div className="page-header">
        <div>
          <div className="page-title">Dashboard</div>
          <div className="page-subtitle">Overview of all your git projects</div>
        </div>
        <Link to="/projects" className="btn btn-primary">
          + Add Project
        </Link>
      </div>

      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-value blue">{stats?.total_projects ?? 0}</div>
          <div className="stat-label">Projects</div>
        </div>
        <div className="stat-card">
          <div className="stat-value green">{stats?.total_branches ?? 0}</div>
          <div className="stat-label">Current Branches</div>
        </div>
        <div className="stat-card">
          <div className="stat-value red">{stats?.total_deleted ?? 0}</div>
          <div className="stat-label">Deleted Branches</div>
        </div>
      </div>

      {stats && stats.projects_summary.length > 0 ? (
        <div className="card">
          <div className="card-header">Projects</div>
          <div className="projects-grid">
            {stats.projects_summary.map((p) => (
              <div className="project-row" key={p.id}>
                <div className="project-info">
                  <div className="project-name">{p.name}</div>
                  <div className="project-path">{p.path}</div>
                </div>
                <div className="project-meta">
                  <div className="project-meta-item">
                    <span>{p.branch_count} branches</span>
                  </div>
                  <div className="project-meta-item">
                    <span className="current-branch-label">{p.current_branch}</span>
                  </div>
                </div>
                <Link to={`/projects/${p.id}`} className="btn btn-ghost">
                  Manage
                </Link>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="card">
          <div className="empty-state">
            <div className="empty-state-icon">◫</div>
            <div className="empty-state-title">No projects yet</div>
            <div className="empty-state-desc">
              Add a git repository to start managing your branches.
            </div>
            <Link to="/projects" className="btn btn-primary" style={{ marginTop: 12 }}>
              Add your first project
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}
