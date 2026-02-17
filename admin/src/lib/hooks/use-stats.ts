import { useQuery } from "@tanstack/react-query";
import { getGlobalStats, getRelayStats } from "../api/stats";

export function useGlobalStats() {
  return useQuery({
    queryKey: ["stats"],
    queryFn: getGlobalStats,
    refetchInterval: 3000,
  });
}

export function useRelayStats(id: string) {
  return useQuery({
    queryKey: ["stats", id],
    queryFn: () => getRelayStats(id),
    refetchInterval: 3000,
    enabled: !!id,
  });
}
