import { apiFetch } from "./client";

export interface StatusResponse {
  pending_restart: boolean;
  domain: string;
  port: number;
}

export async function getStatus(): Promise<StatusResponse> {
  return apiFetch<StatusResponse>("/status");
}

export async function restartServer(): Promise<void> {
  await apiFetch<void>("/restart", { method: "POST" });
}

export interface UpdateStatus {
  status: "idle" | "pulling" | "building" | "complete" | "error";
  message?: string;
  started_at?: string;
  completed_at?: string;
}

export async function triggerUpdate(): Promise<void> {
  await apiFetch<void>("/update", { method: "POST" });
}

export async function getUpdateStatus(): Promise<UpdateStatus> {
  return apiFetch<UpdateStatus>("/update-status");
}
