"use client";

import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { formatSize, formatNumber } from "@/lib/utils/format";
import type { RelayStatsData } from "@/lib/types/stats";

interface RelayStatsTableProps {
  relays?: RelayStatsData[];
  isLoading: boolean;
}

export function RelayStatsTable({ relays, isLoading }: RelayStatsTableProps) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-base">Per-Relay Stats</CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-2">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        ) : !relays || relays.length === 0 ? (
          <p className="text-sm text-muted-foreground">No relays configured</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left text-muted-foreground">
                  <th className="pb-2 pr-4 font-medium">Relay</th>
                  <th className="pb-2 pr-4 font-medium text-right">Conns</th>
                  <th className="pb-2 pr-4 font-medium text-right">Events</th>
                  <th className="pb-2 pr-4 font-medium text-right">Queries</th>
                  <th className="pb-2 pr-4 font-medium text-right">Storage</th>
                  <th className="pb-2 font-medium text-right">RX / TX</th>
                </tr>
              </thead>
              <tbody>
                {relays.map((relay) => (
                  <tr key={relay.relay_id} className="border-b last:border-0">
                    <td className="py-2 pr-4 font-mono text-xs">
                      {relay.relay_id}
                    </td>
                    <td className="py-2 pr-4 text-right tabular-nums">
                      {relay.active_connections}
                    </td>
                    <td className="py-2 pr-4 text-right tabular-nums">
                      {formatNumber(relay.events_stored)}
                    </td>
                    <td className="py-2 pr-4 text-right tabular-nums">
                      {formatNumber(relay.queries_served)}
                    </td>
                    <td className="py-2 pr-4 text-right tabular-nums">
                      {formatSize(relay.storage_bytes)}
                    </td>
                    <td className="py-2 text-right tabular-nums text-xs">
                      {formatSize(relay.bytes_rx)} / {formatSize(relay.bytes_tx)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
