"use client";

import { useGlobalStats } from "@/lib/hooks/use-stats";
import { OverviewCards } from "@/components/stats/overview-cards";
import { RelayStatsTable } from "@/components/stats/relay-stats-table";
import { SystemStatsCard } from "@/components/stats/system-stats-card";

export default function DashboardPage() {
  const { data, isLoading } = useGlobalStats();

  return (
    <div className="space-y-6">
      <OverviewCards data={data} isLoading={isLoading} />
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2">
          <RelayStatsTable relays={data?.relays} isLoading={isLoading} />
        </div>
        <div>
          <SystemStatsCard data={data} isLoading={isLoading} />
        </div>
      </div>
    </div>
  );
}
