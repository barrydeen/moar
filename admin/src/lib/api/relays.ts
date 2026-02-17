import { apiFetch, apiUpload } from "./client";
import type { Relay, RelayConfig, RelayPage, ImportResult } from "../types/relay";

export async function listRelays(): Promise<Relay[]> {
  return apiFetch<Relay[]>("/relays");
}

export async function getRelay(id: string): Promise<Relay> {
  return apiFetch<Relay>(`/relays/${id}`);
}

export async function createRelay(
  id: string,
  config: RelayConfig
): Promise<Relay> {
  return apiFetch<Relay>("/relays", {
    method: "POST",
    body: JSON.stringify({ id, ...config }),
  });
}

export async function updateRelay(
  id: string,
  config: RelayConfig
): Promise<Relay> {
  return apiFetch<Relay>(`/relays/${id}`, {
    method: "PUT",
    body: JSON.stringify(config),
  });
}

export async function deleteRelay(id: string): Promise<void> {
  return apiFetch<void>(`/relays/${id}`, { method: "DELETE" });
}

export async function getRelayPage(id: string): Promise<RelayPage> {
  return apiFetch<RelayPage>(`/relays/${id}/page`);
}

export async function putRelayPage(
  id: string,
  html: string
): Promise<void> {
  return apiFetch<void>(`/relays/${id}/page`, {
    method: "PUT",
    body: JSON.stringify({ html }),
  });
}

export async function deleteRelayPage(id: string): Promise<void> {
  return apiFetch<void>(`/relays/${id}/page`, { method: "DELETE" });
}

export function exportRelayUrl(id: string): string {
  return `/api/relays/${id}/export`;
}

export async function importRelay(id: string, file: File): Promise<ImportResult> {
  const formData = new FormData();
  formData.append("file", file);
  return apiUpload<ImportResult>(`/relays/${id}/import`, formData);
}
