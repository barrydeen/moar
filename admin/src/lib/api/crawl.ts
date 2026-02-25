import { apiFetch } from "./client";
import type { CrawlInfo } from "../types/crawl";

export async function listCrawls(): Promise<CrawlInfo[]> {
  return apiFetch<CrawlInfo[]>("/crawls");
}

export async function getCrawl(id: string): Promise<CrawlInfo> {
  return apiFetch<CrawlInfo>(`/crawls/${id}`);
}

export async function createCrawl(data: {
  id: string;
  relay: string;
  remote_relays: string[];
  authors_from_wot?: string | null;
  authors?: string[] | null;
  kinds?: number[] | null;
  since: number;
  until?: number | null;
  window_hours: number;
  max_requests_per_second: number;
  batch_size: number;
  paused?: boolean;
}): Promise<void> {
  return apiFetch<void>("/crawls", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function updateCrawl(
  id: string,
  data: {
    relay: string;
    remote_relays: string[];
    authors_from_wot?: string | null;
    authors?: string[] | null;
    kinds?: number[] | null;
    since: number;
    until?: number | null;
    window_hours: number;
    max_requests_per_second: number;
    batch_size: number;
    paused?: boolean;
  }
): Promise<void> {
  return apiFetch<void>(`/crawls/${id}`, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export async function deleteCrawl(id: string): Promise<void> {
  return apiFetch<void>(`/crawls/${id}`, { method: "DELETE" });
}

export async function pauseCrawl(id: string): Promise<void> {
  return apiFetch<void>(`/crawls/${id}/pause`, { method: "POST" });
}

export async function resumeCrawl(id: string): Promise<void> {
  return apiFetch<void>(`/crawls/${id}/resume`, { method: "POST" });
}
