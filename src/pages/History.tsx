import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DeletedBranch } from "../types";

export default function History() {
  const [records, setRecords] = useState<DeletedBranch[]>([]);
  const [loading, setLoading] = useState(true);

  const load = () =>
    invoke<DeletedBranch[]>("get_deleted_branches")
      .then((data) => setRecords([...data].reverse()))
      .finally(() => setLoading(false));

  useEffect(() => { load(); }, []);

  const handleClear = async () => {
    if (!confirm("Clear all deletion history? This cannot be undone.")) return;
    await invoke("clear_history");
    load();
  };

  if (loading) return <div className="loading"><div className="spinner" />Loading…</div>;

  return (
    <div>
      <div className="page-header">
        <div>
          <div className="page-title">Deletion History</div>
          <div className="page-subtitle">All branches deleted via GitSweep</div>
        </div>
        {records.length > 0 && (
          <button className="btn btn-ghost" style={{ color: "var(--red-hover)", borderColor: "rgba(218,54,51,0.3)" }} onClick={handleClear}>
            Clear History
          </button>
        )}
      </div>

      {records.length === 0 ? (
        <div className="card">
          <div className="empty-state">
            <div className="empty-state-icon">◷</div>
            <div className="empty-state-title">No deleted branches yet</div>
            <div className="empty-state-desc">
              Branches you delete through GitSweep will appear here.
            </div>
          </div>
        </div>
      ) : (
        <div className="card">
          <div className="card-header">
            {records.length} branch{records.length !== 1 ? "es" : ""} deleted
          </div>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Branch</th>
                  <th>Project</th>
                  <th>Deleted At</th>
                </tr>
              </thead>
              <tbody>
                {records.map((r) => (
                  <tr key={r.id}>
                    <td>
                      <span className="deleted-branch-name">{r.branch_name}</span>
                    </td>
                    <td>
                      <span className="deleted-project">{r.project_name}</span>
                    </td>
                    <td>
                      <span className="deleted-date">
                        {new Date(r.deleted_at).toLocaleString()}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
