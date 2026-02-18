import { apiFetch } from "./client";
import type { PaywallInfo, WhitelistEntry } from "../types/paywall";

export async function listPaywalls(): Promise<PaywallInfo[]> {
  return apiFetch<PaywallInfo[]>("/paywalls");
}

export async function getPaywall(id: string): Promise<PaywallInfo> {
  return apiFetch<PaywallInfo>(`/paywalls/${id}`);
}

export async function createPaywall(data: {
  id: string;
  nwc_string: string;
  price_sats: number;
  period_days: number;
}): Promise<void> {
  return apiFetch<void>("/paywalls", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function updatePaywall(
  id: string,
  data: { nwc_string: string; price_sats: number; period_days: number }
): Promise<void> {
  return apiFetch<void>(`/paywalls/${id}`, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export async function deletePaywall(id: string): Promise<void> {
  return apiFetch<void>(`/paywalls/${id}`, { method: "DELETE" });
}

export async function verifyNwc(
  id: string,
  nwc_string: string
): Promise<void> {
  return apiFetch<void>(`/paywalls/${id}/verify-nwc`, {
    method: "POST",
    body: JSON.stringify({ nwc_string }),
  });
}

export async function getPaywallWhitelist(
  id: string
): Promise<WhitelistEntry[]> {
  return apiFetch<WhitelistEntry[]>(`/paywalls/${id}/whitelist`);
}
