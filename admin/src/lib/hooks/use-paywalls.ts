import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  listPaywalls,
  getPaywall,
  createPaywall,
  updatePaywall,
  deletePaywall,
  getPaywallWhitelist,
} from "../api/paywalls";
import { fetchProfiles } from "@/lib/nostr/pool";
import type { NostrProfile } from "@/lib/types/nostr";
import type { WhitelistEntry } from "@/lib/types/paywall";

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

export function usePaywallWhitelistProfiles(id: string, relays: string[]) {
  return useQuery({
    queryKey: ["paywalls", id, "whitelist-profiles", relays],
    queryFn: async (): Promise<{
      entries: WhitelistEntry[];
      profiles: Map<string, NostrProfile>;
    }> => {
      const entries = await getPaywallWhitelist(id);
      const pubkeys = entries.map((e) => e.pubkey);
      const profiles =
        pubkeys.length > 0 && relays.length > 0
          ? await fetchProfiles(relays, pubkeys)
          : new Map<string, NostrProfile>();
      return { entries, profiles };
    },
    enabled: !!id && relays.length > 0,
    staleTime: 5 * 60 * 1000,
  });
}
