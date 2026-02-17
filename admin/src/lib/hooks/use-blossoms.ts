import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  listBlossoms,
  getBlossom,
  createBlossom,
  updateBlossom,
  deleteBlossom,
} from "../api/blossoms";
import type { BlossomConfig } from "../types/blossom";

export function useBlossoms() {
  return useQuery({
    queryKey: ["blossoms"],
    queryFn: listBlossoms,
  });
}

export function useBlossom(id: string) {
  return useQuery({
    queryKey: ["blossoms", id],
    queryFn: () => getBlossom(id),
    enabled: !!id,
  });
}

export function useCreateBlossom() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, config }: { id: string; config: BlossomConfig }) =>
      createBlossom(id, config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["blossoms"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useUpdateBlossom() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, config }: { id: string; config: BlossomConfig }) =>
      updateBlossom(id, config),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: ["blossoms"] });
      queryClient.invalidateQueries({ queryKey: ["blossoms", id] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useDeleteBlossom() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteBlossom,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["blossoms"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}
