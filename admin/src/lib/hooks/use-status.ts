import { useQuery } from "@tanstack/react-query";
import { getStatus } from "../api/status";

export function useStatus() {
  return useQuery({
    queryKey: ["status"],
    queryFn: getStatus,
    refetchInterval: 5000,
  });
}
