import { apiFetch, apiUpload } from "./client";
import type { Blossom, BlossomConfig, BlobDescriptor } from "../types/blossom";

export async function listBlossoms(): Promise<Blossom[]> {
  return apiFetch<Blossom[]>("/blossoms");
}

export async function getBlossom(id: string): Promise<Blossom> {
  return apiFetch<Blossom>(`/blossoms/${id}`);
}

export async function createBlossom(
  id: string,
  config: BlossomConfig
): Promise<Blossom> {
  return apiFetch<Blossom>("/blossoms", {
    method: "POST",
    body: JSON.stringify({ id, ...config }),
  });
}

export async function updateBlossom(
  id: string,
  config: BlossomConfig
): Promise<Blossom> {
  return apiFetch<Blossom>(`/blossoms/${id}`, {
    method: "PUT",
    body: JSON.stringify(config),
  });
}

export async function deleteBlossom(id: string): Promise<void> {
  return apiFetch<void>(`/blossoms/${id}`, { method: "DELETE" });
}

export async function listMedia(blossomId: string): Promise<BlobDescriptor[]> {
  return apiFetch<BlobDescriptor[]>(`/blossoms/${blossomId}/media`);
}

export async function uploadMedia(
  blossomId: string,
  file: File
): Promise<BlobDescriptor> {
  const formData = new FormData();
  formData.append("file", file);
  return apiUpload<BlobDescriptor>(`/blossoms/${blossomId}/media`, formData);
}

export async function deleteMedia(
  blossomId: string,
  sha256: string
): Promise<void> {
  return apiFetch<void>(`/blossoms/${blossomId}/media/${sha256}`, {
    method: "DELETE",
  });
}
