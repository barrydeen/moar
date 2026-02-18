export interface RelayStatsData {
  relay_id: string;
  active_connections: number;
  total_connections: number;
  events_stored: number;
  events_saved: number;
  events_rejected: number;
  queries_served: number;
  bytes_rx: number;
  bytes_tx: number;
  storage_bytes: number;
}

export interface TimeBucket {
  timestamp: number;
  active_connections: number;
  total_connections: number;
  events_saved: number;
  events_rejected: number;
  queries_served: number;
  bytes_rx: number;
  bytes_tx: number;
  event_count: number;
  storage_bytes: number;
}

export interface SystemStats {
  cpu_usage_percent: number;
  memory_used_bytes: number;
  memory_total_bytes: number;
  disk_used_bytes: number;
  disk_total_bytes: number;
}

export interface GlobalStats {
  uptime_seconds: number;
  total_active_connections: number;
  total_events_stored: number;
  total_storage_bytes: number;
  total_bytes_rx: number;
  total_bytes_tx: number;
  relay_count: number;
  relays: RelayStatsData[];
  system: SystemStats;
}

export interface RelayStatsDetail extends RelayStatsData {
  history: TimeBucket[];
}
