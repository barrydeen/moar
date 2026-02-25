"use client";

import { Skeleton } from "@/components/ui/skeleton";
import { CrawlForm } from "@/components/crawl/crawl-form";
import { useRelays } from "@/lib/hooks/use-relays";
import { useWots } from "@/lib/hooks/use-wot";

export default function NewCrawlPage() {
  const { data: relays, isLoading: relaysLoading } = useRelays();
  const { data: wots, isLoading: wotsLoading } = useWots();

  if (relaysLoading || wotsLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64" />
      </div>
    );
  }

  const relayIds = relays?.map((r) => r.id) ?? [];
  const wotIds = wots?.map((w) => w.id) ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">New Crawl</h2>
      </div>
      <CrawlForm relayIds={relayIds} wotIds={wotIds} />
    </div>
  );
}
