"use client";

import { useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import { WotCard } from "@/components/wot/wot-card";
import { DiscoveryRelays } from "@/components/wot/discovery-relays";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { useWots, useDeleteWot } from "@/lib/hooks/use-wot";
import { Plus } from "lucide-react";
import { toast } from "sonner";

export default function WotPage() {
  const { data: wots, isLoading } = useWots();
  const deleteWot = useDeleteWot();
  const [deleteId, setDeleteId] = useState<string | null>(null);

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await deleteWot.mutateAsync(deleteId);
      toast.success("Web of Trust deleted");
      setDeleteId(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold tracking-tight">Web of Trust</h2>
        <Link href="/admin/wot/new">
          <Button>
            <Plus className="mr-2 h-4 w-4" />
            New WoT
          </Button>
        </Link>
      </div>

      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2">
          {[...Array(2)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : wots?.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <p>No Web of Trust configs yet.</p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {wots?.map((wot) => (
            <WotCard key={wot.id} wot={wot} onDelete={setDeleteId} />
          ))}
        </div>
      )}

      <Separator />
      <DiscoveryRelays />

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
        title="Delete Web of Trust"
        description={`Are you sure you want to delete "${deleteId}"? Relay policies referencing this WoT will stop working.`}
        onConfirm={handleDelete}
        loading={deleteWot.isPending}
      />
    </div>
  );
}
