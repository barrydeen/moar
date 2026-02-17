export function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function truncatePubkey(pubkey: string): string {
  if (pubkey.length <= 16) return pubkey;
  return `${pubkey.slice(0, 8)}...${pubkey.slice(-8)}`;
}

export function formatTimestamp(unix: number): string {
  return new Date(unix * 1000).toLocaleString();
}
