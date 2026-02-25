export interface SyncConfig {
  relay: string;
  remote_relays: string[];
  interval_minutes: number;
  authors: string[] | null;
  authors_from_wot: string | null;
  kinds: number[] | null;
  tags: Record<string, string[]> | null;
  limit: number | null;
}

export type SyncStatus =
  | { state: "Idle" }
  | { state: "Syncing"; relay_url: string; events_so_far: number }
  | { state: "Ready" }
  | { state: "Error"; message: string };

export interface SyncInfo {
  id: string;
  config: SyncConfig;
  status: SyncStatus;
  last_synced: number | null;
  events_pulled: number;
}
