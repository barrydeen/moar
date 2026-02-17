export interface RateLimitConfig {
  writes_per_minute?: number | null;
  reads_per_minute?: number | null;
}

export interface WritePolicy {
  require_auth: boolean;
  allowed_pubkeys?: string[] | null;
  blocked_pubkeys?: string[] | null;
  tagged_pubkeys?: string[] | null;
  wot?: string | null;
}

export interface ReadPolicy {
  require_auth: boolean;
  allowed_pubkeys?: string[] | null;
  wot?: string | null;
}

export interface EventPolicy {
  allowed_kinds?: number[] | null;
  blocked_kinds?: number[] | null;
  min_pow?: number | null;
  max_content_length?: number | null;
}

export interface PolicyConfig {
  write: WritePolicy;
  read: ReadPolicy;
  events: EventPolicy;
  rate_limit?: RateLimitConfig | null;
}

export interface RelayConfig {
  name: string;
  description?: string | null;
  subdomain: string;
  db_path: string;
  policy: PolicyConfig;
}

export interface Relay {
  id: string;
  name: string;
  description?: string | null;
  subdomain: string;
  db_path: string;
  policy: PolicyConfig;
}

export interface RelayPage {
  html: string | null;
}
