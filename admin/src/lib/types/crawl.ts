export interface CrawlConfig {
  relay: string;
  remote_relays: string[];
  authors_from_wot: string | null;
  authors: string[] | null;
  kinds: number[] | null;
  since: number;
  until: number | null;
  window_hours: number;
  max_requests_per_second: number;
  batch_size: number;
  paused: boolean;
}

export interface CrawlProgress {
  total_events: number;
  current_window_start: number;
  current_window_end: number;
  started_at: number;
}

export type CrawlStatus =
  | { state: "Idle" }
  | { state: "Crawling"; window_start: number; window_end: number; events_so_far: number }
  | { state: "Paused" }
  | { state: "Complete" }
  | { state: "Error"; message: string };

export interface CrawlInfo {
  id: string;
  config: CrawlConfig;
  status: CrawlStatus;
  progress: CrawlProgress;
}
