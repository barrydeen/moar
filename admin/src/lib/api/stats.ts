import { apiFetch } from "./client";
import type { GlobalStats, RelayStatsDetail } from "../types/stats";

export async function getGlobalStats(): Promise<GlobalStats> {
  return apiFetch<GlobalStats>("/stats");
}

export async function getRelayStats(id: string): Promise<RelayStatsDetail> {
  return apiFetch<RelayStatsDetail>(`/stats/${id}`);
}
