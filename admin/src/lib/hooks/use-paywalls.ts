import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  listPaywalls,
  getPaywall,
  createPaywall,
  updatePaywall,
  deletePaywall,
  getPaywallWhitelist,
} from "../api/paywalls";

export function usePaywalls() {
  return useQuery({
    queryKey: ["paywalls"],
    queryFn: listPaywalls,
  });
}

export function usePaywall(id: string) {
  return useQuery({
    queryKey: ["paywalls", id],
    queryFn: () => getPaywall(id),
    enabled: !!id,
  });
}

export function useCreatePaywall() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createPaywall,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["paywalls"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useUpdatePaywall() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      ...data
    }: {
      id: string;
      nwc_string: string;
      price_sats: number;
      period_days: number;
    }) => updatePaywall(id, data),
    onSuccess: (_, { id }) => {
      queryClient.invalidateQueries({ queryKey: ["paywalls"] });
      queryClient.invalidateQueries({ queryKey: ["paywalls", id] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function useDeletePaywall() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deletePaywall,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["paywalls"] });
      queryClient.invalidateQueries({ queryKey: ["status"] });
    },
  });
}

export function usePaywallWhitelist(id: string) {
  return useQuery({
    queryKey: ["paywalls", id, "whitelist"],
    queryFn: () => getPaywallWhitelist(id),
    enabled: !!id,
  });
}
