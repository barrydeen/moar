"use client";

import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { formatSize, formatUptime } from "@/lib/utils/format";
import type { GlobalStats } from "@/lib/types/stats";

interface SystemStatsCardProps {
  data?: GlobalStats;
  isLoading: boolean;
}

function StatRow({
  label,
  value,
  isLoading,
}: {
  label: string;
  value: string;
  isLoading: boolean;
}) {
  return (
    <div className="flex justify-between py-1.5">
      <span className="text-sm text-muted-foreground">{label}</span>
      {isLoading ? (
        <Skeleton className="h-5 w-24" />
      ) : (
        <span className="text-sm font-medium tabular-nums">{value}</span>
      )}
    </div>
  );
}

function ProgressBar({ value, max }: { value: number; max: number }) {
  const pct = max > 0 ? Math.min((value / max) * 100, 100) : 0;
  return (
    <div className="h-1.5 w-full rounded-full bg-muted mt-0.5">
      <div
        className="h-full rounded-full bg-primary transition-all"
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

export function SystemStatsCard({ data, isLoading }: SystemStatsCardProps) {
  const sys = data?.system;
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-base">System</CardTitle>
      </CardHeader>
      <CardContent className="space-y-1">
        <StatRow
          label="Uptime"
          value={data ? formatUptime(data.uptime_seconds) : "-"}
          isLoading={isLoading}
        />
        <div>
          <StatRow
            label="Memory"
            value={
              sys
                ? `${formatSize(sys.memory_used_bytes)} / ${formatSize(sys.memory_total_bytes)}`
                : "-"
            }
            isLoading={isLoading}
          />
          {sys && (
            <ProgressBar value={sys.memory_used_bytes} max={sys.memory_total_bytes} />
          )}
        </div>
        <div>
          <StatRow
            label="Disk"
            value={
              sys
                ? `${formatSize(sys.disk_used_bytes)} / ${formatSize(sys.disk_total_bytes)}`
                : "-"
            }
            isLoading={isLoading}
          />
          {sys && (
            <ProgressBar value={sys.disk_used_bytes} max={sys.disk_total_bytes} />
          )}
        </div>
        <StatRow
          label="CPU"
          value={sys ? `${sys.cpu_usage_percent.toFixed(1)}%` : "-"}
          isLoading={isLoading}
        />
        {sys && (
          <ProgressBar value={sys.cpu_usage_percent} max={100} />
        )}
      </CardContent>
    </Card>
  );
}
