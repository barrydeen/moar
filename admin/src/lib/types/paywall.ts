export interface PaywallInfo {
  id: string;
  price_sats: number;
  period_days: number;
  whitelist_count: number;
}

export interface WhitelistEntry {
  pubkey: string;
  expires_at: number;
}
