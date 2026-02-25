"use client";

import { use } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { CrawlForm } from "@/components/crawl/crawl-form";
import { useCrawl } from "@/lib/hooks/use-crawl";
import { useRelays } from "@/lib/hooks/use-relays";
import { useWots } from "@/lib/hooks/use-wot";

export default function EditCrawlPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const { data: crawl, isLoading: crawlLoading } = useCrawl(id);
  const { data: relays, isLoading: relaysLoading } = useRelays();
  const { data: wots, isLoading: wotsLoading } = useWots();

  if (crawlLoading || relaysLoading || wotsLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64" />
      </div>
    );
  }

  if (!crawl) {
    return <p className="text-muted-foreground">Crawl not found.</p>;
  }

  const relayIds = relays?.map((r) => r.id) ?? [];
  const wotIds = wots?.map((w) => w.id) ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Edit Crawl: {crawl.id}</h2>
      </div>
      <CrawlForm crawl={crawl} relayIds={relayIds} wotIds={wotIds} />
    </div>
  );
}
