"use client";

import { useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { SyncCard } from "@/components/sync/sync-card";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { useSyncs, useDeleteSync } from "@/lib/hooks/use-sync";
import { Plus } from "lucide-react";
import { toast } from "sonner";

export default function SyncPage() {
  const { data: syncs, isLoading } = useSyncs();
  const deleteSync = useDeleteSync();
  const [deleteId, setDeleteId] = useState<string | null>(null);

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await deleteSync.mutateAsync(deleteId);
      toast.success("Sync deleted");
      setDeleteId(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold tracking-tight">Relay Sync</h2>
        <Link href="/admin/sync/new">
          <Button>
            <Plus className="mr-2 h-4 w-4" />
            New Sync
          </Button>
        </Link>
      </div>

      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2">
          {[...Array(2)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : syncs?.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <p>No sync configs yet.</p>
          <p className="text-sm mt-1">
            Create a sync to pull events from remote relays into your local relay.
          </p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {syncs?.map((sync) => (
            <SyncCard key={sync.id} sync={sync} onDelete={setDeleteId} />
          ))}
        </div>
      )}

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
        title="Delete Sync"
        description={`Are you sure you want to delete "${deleteId}"? This will stop pulling events from the configured remote relays.`}
        onConfirm={handleDelete}
        loading={deleteSync.isPending}
      />
    </div>
  );
}
