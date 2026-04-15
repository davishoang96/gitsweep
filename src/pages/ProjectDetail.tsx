import { useEffect, useState, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useParams, Link } from "react-router-dom";
import { BranchInfo, DeleteBranchRequest, DeleteFailure, DeleteResult, Project } from "../types";

type Filter = "all" | "merged" | "unmerged";

export default function ProjectDetail() {
  const { id } = useParams<{ id: string }>();
  const [project, setProject] = useState<Project | null>(null);
  const [branches, setBranches] = useState<BranchInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [deleteRemote, setDeleteRemote] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
  const [result, setResult] = useState<{ deleted: string[]; failed: DeleteFailure[] } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [forceList, setForceList] = useState<DeleteFailure[]>([]);
  const [showForce, setShowForce] = useState(false);
  const [baseBranch, setBaseBranch] = useState<string>("HEAD");

  const load = async (base?: string, initial = false) => {
    if (!id) return;
    if (initial) setLoading(true); else setRefreshing(true);
    setError("");
    try {
      const projects = await invoke<Project[]>("get_projects");
      const p = projects.find((x) => x.id === id) ?? null;
      setProject(p);
      if (p) {
        const b = await invoke<BranchInfo[]>("get_branches", {
          projectId: id,
          baseBranch: base ?? baseBranch,
        });
        setBranches(b);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      if (initial) setLoading(false); else setRefreshing(false);
    }
  };

  const handleFetch = async () => {
    if (!id) return;
    setFetching(true);
    setError("");
    try {
      await invoke("fetch_project", { projectId: id });
      await load();
    } catch (err) {
      setError(String(err));
    } finally {
      setFetching(false);
    }
  };

  const handleBaseBranchChange = (branch: string) => {
    setBaseBranch(branch);
    load(branch);
  };

  useEffect(() => { load(undefined, true); }, [id]);

  const visible = useMemo(() => {
    let list = branches;
    if (filter === "merged") list = list.filter((b) => b.is_merged);
    if (filter === "unmerged") list = list.filter((b) => !b.is_merged && !b.is_current);
    if (search.trim()) {
      const q = search.trim().toLowerCase();
      list = list.filter((b) => b.name.toLowerCase().includes(q));
    }
    return list;
  }, [branches, filter, search]);

  const selectableBranches = visible.filter((b) => !b.is_current);

  const toggleSelect = (name: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const toggleAll = () => {
    const names = selectableBranches.map((b) => b.name);
    const allSelected = names.every((n) => selected.has(n));
    if (allSelected) {
      setSelected((prev) => {
        const next = new Set(prev);
        names.forEach((n) => next.delete(n));
        return next;
      });
    } else {
      setSelected((prev) => new Set([...prev, ...names]));
    }
  };

  const selectedBranches = branches.filter((b) => selected.has(b.name));
  const allVisibleSelected =
    selectableBranches.length > 0 && selectableBranches.every((b) => selected.has(b.name));
  const someVisibleSelected = selectableBranches.some((b) => selected.has(b.name));

  const confirmDelete = async (force = false) => {
    if (!id) return;
    setDeleting(true);
    setShowConfirm(false);
    setShowForce(false);

    const payload: DeleteBranchRequest[] = Array.from(selected).map((branch_name) => ({
      branch_name,
      delete_remote: deleteRemote,
    }));

    try {
      const res = await invoke<DeleteResult>("delete_branches", {
        projectId: id,
        branches: payload,
        force,
      });

      setResult({ deleted: res.deleted, failed: res.failed });

      // Clear selected branches that were successfully deleted
      setSelected((prev) => {
        const next = new Set(prev);
        res.deleted.forEach((n) => next.delete(n));
        return next;
      });

      const needsForce = res.failed.filter((f) => f.needs_force);
      if (needsForce.length > 0) {
        setForceList(needsForce);
        setShowForce(true);
      }

      await load();
    } catch (err) {
      setResult({ deleted: [], failed: [{ branch: "all", error: String(err), needs_force: false }] });
    } finally {
      setDeleting(false);
    }
  };

  if (loading) return <div className="loading"><div className="spinner" />Loading…</div>;
  if (!project) return <div className="error-msg">Project not found.</div>;

  return (
    <div>
      <div className="page-header">
        <div>
          <div className="page-title">{project.name}</div>
          <div className="page-subtitle" style={{ fontFamily: "monospace" }}>{project.path}</div>
        </div>
          <span style={{ marginLeft: "auto", fontSize: 13, color: "var(--text-muted)" }}>
          {branches.length} branch{branches.length !== 1 ? "es" : ""}
        </span>
        
      </div>

      {error && <div className="error-msg">{error}</div>}

      {result && (
        <div>
          {result.deleted.length > 0 && (
            <div className="result-banner success">
              Deleted {result.deleted.length} branch{result.deleted.length > 1 ? "es" : ""}:{" "}
              {result.deleted.join(", ")}
            </div>
          )}
          {result.failed.filter((f) => !f.needs_force).length > 0 && (
            <div className="result-banner error">
              {result.failed.filter((f) => !f.needs_force).map((f) => (
                <div key={f.branch}><strong>{f.branch}</strong>: {f.error}</div>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="toolbar">
        <div className="filter-tabs">
          {(["all", "merged", "unmerged"] as Filter[]).map((f) => (
            <button
              key={f}
              className={"filter-tab" + (filter === f ? " active" : "")}
              onClick={() => setFilter(f)}
            >
              {f.charAt(0).toUpperCase() + f.slice(1)}
              {f === "merged" && (
                <span style={{ marginLeft: 6, color: "var(--text-muted)" }}>
                  ({branches.filter((b) => b.is_merged).length})
                </span>
              )}
            </button>
          ))}
        </div>
        <input
          className="search-input"
          placeholder="Search branches…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
 
        <div style={{ display: "flex", gap: 8, alignItems: "center", marginLeft: "auto" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, color: "var(--text-muted)" }}>
            Merged into:
            <select
              className="base-branch-select"
              value={baseBranch}
              onChange={(e) => handleBaseBranchChange(e.target.value)}
            >
              <option value="HEAD">HEAD (current)</option>
              {branches
                .filter((b) => b.name !== "HEAD")
                .map((b) => (
                  <option key={b.name} value={b.name}>{b.name}</option>
                ))}
            </select>
          </div>
          {refreshing && <span className="refresh-indicator"><span className="spinner" />Updating…</span>}
          <button className="btn btn-ghost" disabled={fetching || refreshing} onClick={handleFetch}>
            {fetching ? "Fetching…" : "⬇ Fetch"}
          </button>
          <button className="btn btn-ghost" disabled={refreshing} onClick={() => load()}>↺ Refresh</button>
        </div>
      </div>

      <div className="card">
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th className="checkbox-col">
                  <input
                    type="checkbox"
                    checked={allVisibleSelected}
                    ref={(el) => { if (el) el.indeterminate = someVisibleSelected && !allVisibleSelected; }}
                    onChange={toggleAll}
                    disabled={selectableBranches.length === 0}
                  />
                </th>
                <th>Branch</th>
                <th>Last Commit</th>
                <th>Date</th>
                <th>Upstream</th>
              </tr>
            </thead>
            <tbody>
              {visible.length === 0 ? (
                <tr>
                  <td colSpan={5}>
                    <div className="empty-state" style={{ padding: "40px 20px" }}>
                      <div className="empty-state-title">No branches found</div>
                    </div>
                  </td>
                </tr>
              ) : (
                visible.map((b) => (
                  <tr
                    key={b.name}
                    className={b.is_current ? "is-current" : ""}
                    onClick={() => !b.is_current && toggleSelect(b.name)}
                    style={{ cursor: b.is_current ? "default" : "pointer" }}
                  >
                    <td className="checkbox-col" onClick={(e) => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        checked={selected.has(b.name)}
                        disabled={b.is_current}
                        onChange={() => toggleSelect(b.name)}
                      />
                    </td>
                    <td>
                      <div className="branch-name-cell">
                        <span className="branch-name">{b.name}</span>
                        {b.is_current && <span className="badge badge-current">current</span>}
                        {b.is_merged && <span className="badge badge-merged">merged</span>}
                      </div>
                    </td>
                    <td>
                      <div className="commit-info">
                        <span className="commit-hash">{b.last_commit_hash}</span>
                        <span className="commit-msg">{b.last_commit_message}</span>
                      </div>
                    </td>
                    <td style={{ color: "var(--text-muted)", fontSize: 12, whiteSpace: "nowrap" }}>
                      {b.last_commit_date}
                    </td>
                    <td style={{ color: "var(--text-muted)", fontSize: 12, fontFamily: "monospace" }}>
                      {b.upstream ?? "—"}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>

        {selected.size > 0 && (
          <div className="action-bar">
            <div className="action-bar-info">
              <span>{selected.size}</span> branch{selected.size > 1 ? "es" : ""} selected
            </div>
            <label className="modal-label" style={{ margin: 0 }}>
              <input
                type="checkbox"
                checked={deleteRemote}
                onChange={(e) => setDeleteRemote(e.target.checked)}
              />
              Also delete remote branches
            </label>
            <button
              className="btn btn-danger"
              disabled={deleting}
              onClick={() => setShowConfirm(true)}
            >
              {deleting ? "Deleting…" : `Delete ${selected.size} Branch${selected.size > 1 ? "es" : ""}`}
            </button>
          </div>
        )}
      </div>

      {showConfirm && (
        <ConfirmDeleteModal
          branches={selectedBranches}
          deleteRemote={deleteRemote}
          setDeleteRemote={setDeleteRemote}
          onConfirm={() => confirmDelete(false)}
          onCancel={() => setShowConfirm(false)}
        />
      )}

      {showForce && (
        <ForceDeleteModal
          failures={forceList}
          onForce={() => {
            // Select only the force-needed branches
            setSelected(new Set(forceList.map((f) => f.branch)));
            setShowForce(false);
            confirmDelete(true);
          }}
          onCancel={() => setShowForce(false)}
        />
      )}
    </div>
  );
}

function ConfirmDeleteModal({
  branches,
  deleteRemote,
  setDeleteRemote,
  onConfirm,
  onCancel,
}: {
  branches: BranchInfo[];
  deleteRemote: boolean;
  setDeleteRemote: (v: boolean) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const hasUpstream = branches.some((b) => b.upstream);
  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Delete Branches</div>
        <div className="modal-body">
          <p>
            You are about to delete <strong>{branches.length} branch{branches.length > 1 ? "es" : ""}</strong>:
          </p>
          <ul className="modal-branch-list">
            {branches.map((b) => (
              <li key={b.name}>{b.name}{b.upstream ? ` → ${b.upstream}` : ""}</li>
            ))}
          </ul>
          {hasUpstream && (
            <label className="modal-label">
              <input
                type="checkbox"
                checked={deleteRemote}
                onChange={(e) => setDeleteRemote(e.target.checked)}
              />
              Also delete remote tracking branches
            </label>
          )}
          <div className="modal-warning">
            This action cannot be undone for force-deleted branches.
            Merged branches can typically be recovered via reflog.
          </div>
        </div>
        <div className="modal-footer">
          <button className="btn btn-ghost" onClick={onCancel}>Cancel</button>
          <button className="btn btn-danger" onClick={onConfirm}>Delete</button>
        </div>
      </div>
    </div>
  );
}

function ForceDeleteModal({
  failures,
  onForce,
  onCancel,
}: {
  failures: DeleteFailure[];
  onForce: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Branches Not Fully Merged</div>
        <div className="modal-body">
          <p>
            The following branches have unmerged commits. Force delete them anyway?
          </p>
          <ul className="force-list" style={{ marginTop: 12 }}>
            {failures.map((f) => (
              <li key={f.branch}>
                <span style={{ fontFamily: "monospace", color: "var(--text)" }}>{f.branch}</span>
                <div className="err-detail">{f.error}</div>
              </li>
            ))}
          </ul>
          <div className="modal-warning" style={{ marginTop: 12 }}>
            Force delete will permanently remove unmerged commits. Make sure you no longer need them.
          </div>
        </div>
        <div className="modal-footer">
          <button className="btn btn-ghost" onClick={onCancel}>Skip</button>
          <button className="btn btn-danger" onClick={onForce}>Force Delete</button>
        </div>
      </div>
    </div>
  );
}
