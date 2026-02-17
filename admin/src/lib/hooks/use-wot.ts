import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  listWots,
  getWot,
  createWot,
  updateWot,
  deleteWot,
  getDiscoveryRelays,
  putDiscoveryRelays,
} from "../api/wot";

export function useWots() {
  return useQuery({
    queryKey: ["wots"],
    queryFn: listWots,
    refetchInterval: 10000,
  });
}

export function useWot(id: string) {
  return useQuery({
    queryKey: ["wots", id],
    queryFn: () => getWot(id),
    enabled: !!id,
  });
}

export function useCreateWot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createWot,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["wots"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useUpdateWot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      ...data
    }: {
      id: string;
      seed: string;
      depth: number;
      update_interval_hours: number;
    }) => updateWot(id, data),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: ["wots"] });
      queryClient.invalidateQueries({ queryKey: ["wots", id] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useDeleteWot() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteWot,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["wots"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useDiscoveryRelays() {
  return useQuery({
    queryKey: ["discovery-relays"],
    queryFn: getDiscoveryRelays,
  });
}

export function usePutDiscoveryRelays() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: putDiscoveryRelays,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["discovery-relays"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}
