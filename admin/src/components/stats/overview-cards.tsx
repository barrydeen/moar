"use client";

import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { formatSize, formatNumber } from "@/lib/utils/format";
import type { GlobalStats } from "@/lib/types/stats";

interface OverviewCardsProps {
  data?: GlobalStats;
  isLoading: boolean;
}

function MetricCard({
  label,
  value,
  isLoading,
}: {
  label: string;
  value: string;
  isLoading: boolean;
}) {
  return (
    <Card>
      <CardContent className="p-4">
        <p className="text-sm text-muted-foreground">{label}</p>
        {isLoading ? (
          <Skeleton className="h-8 w-24 mt-1" />
        ) : (
          <p className="text-2xl font-bold mt-1">{value}</p>
        )}
      </CardContent>
    </Card>
  );
}

export function OverviewCards({ data, isLoading }: OverviewCardsProps) {
  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
      <MetricCard
        label="Connections"
        value={data ? formatNumber(data.total_active_connections) : "0"}
        isLoading={isLoading}
      />
      <MetricCard
        label="Events Stored"
        value={data ? formatNumber(data.total_events_stored) : "0"}
        isLoading={isLoading}
      />
      <MetricCard
        label="Storage"
        value={data ? formatSize(data.total_storage_bytes) : "0 B"}
        isLoading={isLoading}
      />
      <MetricCard
        label="Bandwidth"
        value={
          data
            ? formatSize(data.total_bytes_rx + data.total_bytes_tx)
            : "0 B"
        }
        isLoading={isLoading}
      />
      <MetricCard
        label="Conns Refused"
        value={data ? formatNumber(data.total_connections_refused) : "0"}
        isLoading={isLoading}
      />
      <MetricCard
        label="Write-Limited"
        value={data ? formatNumber(data.total_rate_limited_writes) : "0"}
        isLoading={isLoading}
      />
      <MetricCard
        label="Read-Limited"
        value={data ? formatNumber(data.total_rate_limited_reads) : "0"}
        isLoading={isLoading}
      />
      <MetricCard
        label="Msg Too Large"
        value={data ? formatNumber(data.total_messages_too_large) : "0"}
        isLoading={isLoading}
      />
    </div>
  );
}
