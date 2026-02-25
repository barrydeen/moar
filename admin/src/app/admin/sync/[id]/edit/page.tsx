"use client";

import { use } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { SyncForm } from "@/components/sync/sync-form";
import { useSync } from "@/lib/hooks/use-sync";
import { useRelays } from "@/lib/hooks/use-relays";
import { useWots } from "@/lib/hooks/use-wot";

export default function EditSyncPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const { data: sync, isLoading: syncLoading } = useSync(id);
  const { data: relays, isLoading: relaysLoading } = useRelays();
  const { data: wots, isLoading: wotsLoading } = useWots();

  if (syncLoading || relaysLoading || wotsLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64" />
      </div>
    );
  }

  if (!sync) {
    return <p className="text-muted-foreground">Sync not found.</p>;
  }

  const relayIds = relays?.map((r) => r.id) ?? [];
  const wotIds = wots?.map((w) => w.id) ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Edit Sync: {sync.id}</h2>
      </div>
      <SyncForm sync={sync} relayIds={relayIds} wotIds={wotIds} />
    </div>
  );
}
