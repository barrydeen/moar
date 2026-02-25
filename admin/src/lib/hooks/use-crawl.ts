import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  listCrawls,
  getCrawl,
  createCrawl,
  updateCrawl,
  deleteCrawl,
  pauseCrawl,
  resumeCrawl,
} from "../api/crawl";

export function useCrawls() {
  return useQuery({
    queryKey: ["crawls"],
    queryFn: listCrawls,
    refetchInterval: 5000,
  });
}

export function useCrawl(id: string) {
  return useQuery({
    queryKey: ["crawls", id],
    queryFn: () => getCrawl(id),
    enabled: !!id,
    refetchInterval: 5000,
  });
}

export function useCreateCrawl() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createCrawl,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["crawls"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useUpdateCrawl() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      ...data
    }: {
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
    }) => updateCrawl(id, data),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: ["crawls"] });
      queryClient.invalidateQueries({ queryKey: ["crawls", id] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useDeleteCrawl() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteCrawl,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["crawls"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function usePauseCrawl() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: pauseCrawl,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["crawls"] });
    },
  });
}

export function useResumeCrawl() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: resumeCrawl,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["crawls"] });
    },
  });
}
