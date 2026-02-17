"use client";

import { useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { RelayCard } from "@/components/relays/relay-card";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { useRelays, useDeleteRelay } from "@/lib/hooks/use-relays";
import { useStatus } from "@/lib/hooks/use-status";
import { Plus } from "lucide-react";
import { toast } from "sonner";

export default function RelaysPage() {
  const { data: relays, isLoading } = useRelays();
  const { data: status } = useStatus();
  const deleteRelay = useDeleteRelay();
  const [deleteId, setDeleteId] = useState<string | null>(null);

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await deleteRelay.mutateAsync(deleteId);
      toast.success("Relay deleted");
      setDeleteId(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete relay");
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold tracking-tight">Relays</h2>
        <Link href="/admin/relays/new">
          <Button>
            <Plus className="mr-2 h-4 w-4" />
            New Relay
          </Button>
        </Link>
      </div>

      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2">
          {[...Array(3)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : relays?.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <p>No relays configured yet.</p>
          <p className="text-sm mt-1">Create your first relay to get started.</p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {relays?.map((relay) => (
            <RelayCard
              key={relay.id}
              relay={relay}
              onDelete={setDeleteId}
              domain={status?.domain}
            />
          ))}
        </div>
      )}

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
        title="Delete Relay"
        description={`Are you sure you want to delete relay "${deleteId}"? This will also remove its custom page.`}
        onConfirm={handleDelete}
        loading={deleteRelay.isPending}
      />
    </div>
  );
}
