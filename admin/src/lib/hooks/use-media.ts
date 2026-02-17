import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { listMedia, uploadMedia, deleteMedia } from "../api/blossoms";

export function useMedia(blossomId: string) {
  return useQuery({
    queryKey: ["blossoms", blossomId, "media"],
    queryFn: () => listMedia(blossomId),
    enabled: !!blossomId,
  });
}

export function useUploadMedia(blossomId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (file: File) => uploadMedia(blossomId, file),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["blossoms", blossomId, "media"],
      });
    },
  });
}

export function useDeleteMedia(blossomId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (sha256: string) => deleteMedia(blossomId, sha256),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["blossoms", blossomId, "media"],
      });
    },
  });
}
