import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  listRelays,
  getRelay,
  createRelay,
  updateRelay,
  deleteRelay,
  getRelayPage,
  putRelayPage,
  deleteRelayPage,
  importRelay,
} from "../api/relays";
import type { RelayConfig } from "../types/relay";

export function useRelays() {
  return useQuery({
    queryKey: ["relays"],
    queryFn: listRelays,
  });
}

export function useRelay(id: string) {
  return useQuery({
    queryKey: ["relays", id],
    queryFn: () => getRelay(id),
    enabled: !!id,
  });
}

export function useCreateRelay() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, config }: { id: string; config: RelayConfig }) =>
      createRelay(id, config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["relays"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useUpdateRelay() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, config }: { id: string; config: RelayConfig }) =>
      updateRelay(id, config),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: ["relays"] });
      queryClient.invalidateQueries({ queryKey: ["relays", id] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useDeleteRelay() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteRelay,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["relays"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useRelayPage(id: string) {
  return useQuery({
    queryKey: ["relays", id, "page"],
    queryFn: () => getRelayPage(id),
    enabled: !!id,
  });
}

export function usePutRelayPage() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, html }: { id: string; html: string }) =>
      putRelayPage(id, html),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: ["relays", id, "page"] });
    },
  });
}

export function useDeleteRelayPage() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteRelayPage,
    onSuccess: (_, id) => {
      queryClient.invalidateQueries({ queryKey: ["relays", id, "page"] });
    },
  });
}

export function useImportRelay() {
  return useMutation({
    mutationFn: ({ id, file }: { id: string; file: File }) =>
      importRelay(id, file),
  });
}
