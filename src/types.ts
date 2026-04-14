export interface Project {
  id: string;
  name: string;
  path: string;
  created_at: string;
}

export interface BranchInfo {
  name: string;
  is_current: boolean;
  last_commit_hash: string;
  last_commit_message: string;
  last_commit_date: string;
  is_merged: boolean;
  upstream: string | null;
}

export interface ProjectSummary {
  id: string;
  name: string;
  path: string;
  branch_count: number;
  current_branch: string;
}

export interface DashboardStats {
  total_projects: number;
  total_branches: number;
  total_deleted: number;
  projects_summary: ProjectSummary[];
}

export interface DeletedBranch {
  id: string;
  project_id: string;
  project_name: string;
  branch_name: string;
  deleted_at: string;
}

export interface DeleteBranchRequest {
  branch_name: string;
  delete_remote: boolean;
}

export interface DeleteFailure {
  branch: string;
  error: string;
  needs_force: boolean;
}

export interface DeleteResult {
  deleted: string[];
  failed: DeleteFailure[];
}
