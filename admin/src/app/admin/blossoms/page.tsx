"use client";

import { useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { BlossomCard } from "@/components/blossoms/blossom-card";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { useBlossoms, useDeleteBlossom } from "@/lib/hooks/use-blossoms";
import { useStatus } from "@/lib/hooks/use-status";
import { Plus } from "lucide-react";
import { toast } from "sonner";

export default function BlossomsPage() {
  const { data: blossoms, isLoading } = useBlossoms();
  const { data: status } = useStatus();
  const deleteBlossom = useDeleteBlossom();
  const [deleteId, setDeleteId] = useState<string | null>(null);

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await deleteBlossom.mutateAsync(deleteId);
      toast.success("Blossom server deleted");
      setDeleteId(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold tracking-tight">Blossom Servers</h2>
        <Link href="/admin/blossoms/new">
          <Button>
            <Plus className="mr-2 h-4 w-4" />
            New Server
          </Button>
        </Link>
      </div>

      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2">
          {[...Array(2)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : blossoms?.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <p>No Blossom servers configured yet.</p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {blossoms?.map((b) => (
            <BlossomCard
              key={b.id}
              blossom={b}
              onDelete={setDeleteId}
              domain={status?.domain}
            />
          ))}
        </div>
      )}

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
        title="Delete Blossom Server"
        description={`Are you sure you want to delete "${deleteId}"? Stored media files will not be deleted from disk.`}
        onConfirm={handleDelete}
        loading={deleteBlossom.isPending}
      />
    </div>
  );
}
