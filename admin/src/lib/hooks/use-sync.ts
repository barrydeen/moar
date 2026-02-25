import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  listSyncs,
  getSync,
  createSync,
  updateSync,
  deleteSync,
  triggerSync,
} from "../api/sync";

export function useSyncs() {
  return useQuery({
    queryKey: ["syncs"],
    queryFn: listSyncs,
    refetchInterval: 10000,
  });
}

export function useSync(id: string) {
  return useQuery({
    queryKey: ["syncs", id],
    queryFn: () => getSync(id),
    enabled: !!id,
    refetchInterval: 5000,
  });
}

export function useCreateSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createSync,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["syncs"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useUpdateSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      ...data
    }: {
      id: string;
      relay: string;
      remote_relays: string[];
      interval_minutes: number;
      authors?: string[] | null;
      authors_from_wot?: string | null;
      kinds?: number[] | null;
      tags?: Record<string, string[]> | null;
      limit?: number | null;
    }) => updateSync(id, data),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: ["syncs"] });
      queryClient.invalidateQueries({ queryKey: ["syncs", id] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useDeleteSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteSync,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["syncs"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useTriggerSync() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: triggerSync,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["syncs"] });
    },
  });
}
