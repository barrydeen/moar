export interface WotConfig {
  seed: string;
  depth: number;
  update_interval_hours: number;
}

export type WotStatus =
  | "Pending"
  | { Building: { depth_progress: number; total_depth: number } }
  | "Ready"
  | { Error: { message: string } };

export interface WotInfo {
  id: string;
  config: WotConfig;
  status: WotStatus;
  pubkey_count: number;
  last_updated: number | null;
}
