/**
 * API client for the CSVoyant backend.
 *
 * Two things shape this file:
 *  - Every endpoint answers with the uniform envelope `{ data, error }`, so `unwrap` is the
 *    single place that turns a response into a value or throws.
 *  - Auth is an in-memory access token + an httpOnly refresh cookie (DECISIONS #9). The token
 *    never touches localStorage; on a 401 we transparently rotate via `/auth/refresh` (which
 *    authenticates with the cookie) and replay the request once.
 */

export const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

export type ApiError = { code: string; message: string };
type Envelope<T> = { data: T | null; error: ApiError | null };

/** A failed API call, carrying the server's structured error. */
export class ApiException extends Error {
  constructor(
    public status: number,
    public error: ApiError,
  ) {
    super(error.message);
    this.name = "ApiException";
  }
}

// ── domain types (mirror the Rust DTOs) ──────────────────────────────────────

export type Role = "user" | "admin";
export type JobStatus =
  | "queued"
  | "downloading"
  | "inferring"
  | "ingesting"
  | "ready"
  | "failed";

export type Tokens = {
  access_token: string;
  refresh_token: string;
  expires_in: number;
};
export type User = { id: string; email: string; role: Role };
export type Job = {
  id: string;
  source_url: string;
  status: JobStatus;
  error: string | null;
  row_count: number | null;
  created_at: string;
  finished_at: string | null;
};
export type ColumnKind = "numeric" | "temporal" | "boolean" | "categorical";
export type ColumnMeta = {
  name: string;
  type: string;
  kind: ColumnKind;
  stats: {
    nulls?: number;
    distinct?: number;
    min?: number | string | null;
    max?: number | string | null;
    avg?: number | null;
  };
};
export type ChartSpec = {
  kind: "bar" | "histogram" | "time_series";
  column: string;
  title: string;
  top_values?: { value: string; count: number }[];
};
export type Dashboard = {
  summary: { row_count: number; column_count: number };
  columns: ColumnMeta[];
  charts: ChartSpec[];
};
export type DataPage = {
  rows: Record<string, unknown>[];
  page: number;
  page_size: number;
  total: number;
};

// ── access token (in memory only) ────────────────────────────────────────────

let accessToken: string | null = null;
export const setAccessToken = (t: string | null) => {
  accessToken = t;
};
export const getAccessToken = () => accessToken;

// ── transport ────────────────────────────────────────────────────────────────

function request(path: string, init: RequestInit): Promise<Response> {
  const headers = new Headers(init.headers);
  if (init.body && !headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }
  if (accessToken) headers.set("authorization", `Bearer ${accessToken}`);
  // `credentials: include` sends the httpOnly refresh cookie (API allows this origin via CORS).
  return fetch(`${API_URL}${path}`, { ...init, headers, credentials: "include" });
}

async function unwrap<T>(res: Response): Promise<T> {
  const body = (await res.json().catch(() => null)) as Envelope<T> | null;
  if (!res.ok || body?.error) {
    throw new ApiException(
      res.status,
      body?.error ?? { code: "unknown", message: res.statusText || "Request failed" },
    );
  }
  return body?.data as T;
}

/** Exchange the refresh cookie for a new access token. Returns false if not signed in. */
export async function refresh(): Promise<boolean> {
  try {
    const res = await fetch(`${API_URL}/auth/refresh`, {
      method: "POST",
      credentials: "include",
      headers: { "content-type": "application/json" },
      body: "{}",
    });
    if (!res.ok) return false;
    const body = (await res.json()) as Envelope<Tokens>;
    if (!body.data) return false;
    setAccessToken(body.data.access_token);
    return true;
  } catch {
    return false;
  }
}

/** Call the API; on 401 rotate the token once and replay. */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  let res = await request(path, init);
  if (res.status === 401 && path !== "/auth/refresh") {
    if (await refresh()) res = await request(path, init);
  }
  return unwrap<T>(res);
}

// ── endpoints ────────────────────────────────────────────────────────────────

export const register = (email: string, password: string) =>
  api<Tokens>("/auth/register", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });

export const login = (email: string, password: string) =>
  api<Tokens>("/auth/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });

export const me = () => api<User>("/auth/me");

export const changeEmail = (new_email: string, current_password: string) =>
  api<User>("/auth/email", {
    method: "PATCH",
    body: JSON.stringify({ new_email, current_password }),
  });

export const listJobs = () => api<Job[]>("/jobs");

export const getJob = (id: string) => api<Job>(`/jobs/${id}`);

export const createJob = (url: string) =>
  api<{ job_id: string; status: JobStatus }>("/jobs", {
    method: "POST",
    body: JSON.stringify({ url }),
  });

export const getDashboard = (id: string) => api<Dashboard>(`/jobs/${id}/dashboard`);

export function getData(
  id: string,
  opts: { page?: number; pageSize?: number; sort?: string; order?: "asc" | "desc" } = {},
) {
  const q = new URLSearchParams();
  if (opts.page) q.set("page", String(opts.page));
  if (opts.pageSize) q.set("page_size", String(opts.pageSize));
  if (opts.sort) q.set("sort", opts.sort);
  if (opts.order) q.set("order", opts.order);
  const qs = q.toString();
  return api<DataPage>(`/jobs/${id}/data${qs ? `?${qs}` : ""}`);
}

/** A job is still moving until it reaches a terminal state. */
export const isTerminal = (s: JobStatus) => s === "ready" || s === "failed";
