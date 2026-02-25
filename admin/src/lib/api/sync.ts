import { apiFetch } from "./client";
import type { SyncInfo } from "../types/sync";

export async function listSyncs(): Promise<SyncInfo[]> {
  return apiFetch<SyncInfo[]>("/syncs");
}

export async function getSync(id: string): Promise<SyncInfo> {
  return apiFetch<SyncInfo>(`/syncs/${id}`);
}

export async function createSync(data: {
  id: string;
  relay: string;
  remote_relays: string[];
  interval_minutes: number;
  authors?: string[] | null;
  authors_from_wot?: string | null;
  kinds?: number[] | null;
  tags?: Record<string, string[]> | null;
  limit?: number | null;
}): Promise<void> {
  return apiFetch<void>("/syncs", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function updateSync(
  id: string,
  data: {
    relay: string;
    remote_relays: string[];
    interval_minutes: number;
    authors?: string[] | null;
    authors_from_wot?: string | null;
    kinds?: number[] | null;
    tags?: Record<string, string[]> | null;
    limit?: number | null;
  }
): Promise<void> {
  return apiFetch<void>(`/syncs/${id}`, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export async function deleteSync(id: string): Promise<void> {
  return apiFetch<void>(`/syncs/${id}`, { method: "DELETE" });
}

export async function triggerSync(id: string): Promise<void> {
  return apiFetch<void>(`/syncs/${id}/trigger`, { method: "POST" });
}
