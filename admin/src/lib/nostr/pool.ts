import { SimplePool } from "nostr-tools/pool";
import type { NostrProfile } from "@/lib/types/nostr";

let pool: SimplePool | null = null;

export function getPool(): SimplePool {
  if (!pool) {
    pool = new SimplePool();
  }
  return pool;
}

export async function fetchContacts(
  relays: string[],
  pubkey: string
): Promise<string[]> {
  const p = getPool();

  try {
    const events = await p.querySync(relays, {
      kinds: [3],
      authors: [pubkey],
    }, { maxWait: 8000 });

    if (events.length === 0) return [];

    // Pick the most recent kind 3 event
    const latest = events.reduce((a, b) =>
      a.created_at > b.created_at ? a : b
    );

    return latest.tags
      .filter((tag) => tag[0] === "p" && tag[1])
      .map((tag) => tag[1]);
  } catch (err) {
    console.warn("fetchContacts failed:", err);
    return [];
  }
}

export async function fetchProfiles(
  relays: string[],
  pubkeys: string[]
): Promise<Map<string, NostrProfile>> {
  const p = getPool();
  const profiles = new Map<string, NostrProfile>();
  const BATCH_SIZE = 50;

  for (let i = 0; i < pubkeys.length; i += BATCH_SIZE) {
    const batch = pubkeys.slice(i, i + BATCH_SIZE);

    try {
      const events = await p.querySync(relays, {
        kinds: [0],
        authors: batch,
      }, { maxWait: 8000 });

      // Keep only the most recent event per pubkey
      const latest = new Map<string, { created_at: number; content: string }>();
      for (const event of events) {
        const existing = latest.get(event.pubkey);
        if (!existing || event.created_at > existing.created_at) {
          latest.set(event.pubkey, {
            created_at: event.created_at,
            content: event.content,
          });
        }
      }

      for (const [pubkey, { content }] of latest) {
        try {
          const parsed = JSON.parse(content);
          profiles.set(pubkey, {
            pubkey,
            name: parsed.name,
            display_name: parsed.display_name,
            picture: parsed.picture,
            about: parsed.about,
            nip05: parsed.nip05,
          });
        } catch {
          profiles.set(pubkey, { pubkey });
        }
      }
    } catch (err) {
      console.warn(`fetchProfiles batch ${i} failed:`, err);
    }
  }

  return profiles;
}
