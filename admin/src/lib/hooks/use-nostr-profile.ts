import { useQuery } from "@tanstack/react-query";
import { fetchContacts, fetchProfiles } from "@/lib/nostr/pool";
import { hasNostrExtension } from "@/lib/utils/nostr";
import type { NostrProfile } from "@/lib/types/nostr";

export function useAdminPubkey() {
  return useQuery({
    queryKey: ["admin-pubkey"],
    queryFn: async () => {
      if (!hasNostrExtension()) throw new Error("No NIP-07 extension");
      return window.nostr!.getPublicKey();
    },
    enabled: typeof window !== "undefined" && hasNostrExtension(),
    staleTime: Infinity,
    retry: false,
  });
}

export function useNostrProfile(
  pubkey: string | undefined,
  relays: string[] | undefined
) {
  return useQuery({
    queryKey: ["nostr-profile", pubkey],
    queryFn: async (): Promise<NostrProfile | null> => {
      if (!pubkey || !relays?.length) return null;
      const profiles = await fetchProfiles(relays, [pubkey]);
      return profiles.get(pubkey) ?? { pubkey };
    },
    enabled: !!pubkey && !!relays?.length,
    staleTime: 5 * 60 * 1000,
  });
}

export function useWotFollows(seed: string, relays: string[]) {
  return useQuery({
    queryKey: ["wot-follows", seed],
    queryFn: async () => {
      const contacts = await fetchContacts(relays, seed);
      const profiles =
        contacts.length > 0
          ? await fetchProfiles(relays, contacts)
          : new Map<string, NostrProfile>();
      return { contacts, profiles };
    },
    staleTime: 5 * 60 * 1000,
  });
}
