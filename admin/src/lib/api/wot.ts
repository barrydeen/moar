import { apiFetch } from "./client";
import type { WotInfo } from "../types/wot";

export async function listWots(): Promise<WotInfo[]> {
  return apiFetch<WotInfo[]>("/wots");
}

export async function getWot(id: string): Promise<WotInfo> {
  return apiFetch<WotInfo>(`/wots/${id}`);
}

export async function createWot(data: {
  id: string;
  seed: string;
  depth: number;
  update_interval_hours: number;
}): Promise<void> {
  return apiFetch<void>("/wots", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function updateWot(
  id: string,
  data: { seed: string; depth: number; update_interval_hours: number }
): Promise<void> {
  return apiFetch<void>(`/wots/${id}`, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export async function deleteWot(id: string): Promise<void> {
  return apiFetch<void>(`/wots/${id}`, { method: "DELETE" });
}

export async function getDiscoveryRelays(): Promise<string[]> {
  return apiFetch<string[]>("/discovery-relays");
}

export async function putDiscoveryRelays(relays: string[]): Promise<void> {
  return apiFetch<void>("/discovery-relays", {
    method: "PUT",
    body: JSON.stringify({ relays }),
  });
}
