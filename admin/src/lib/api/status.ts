import { apiFetch } from "./client";

export interface StatusResponse {
  pending_restart: boolean;
  domain: string;
  port: number;
}

export async function getStatus(): Promise<StatusResponse> {
  return apiFetch<StatusResponse>("/status");
}
