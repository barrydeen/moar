"use client";

import { useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { PaywallCard } from "@/components/paywalls/paywall-card";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { usePaywalls, useDeletePaywall } from "@/lib/hooks/use-paywalls";
import { Plus } from "lucide-react";
import { toast } from "sonner";

export default function PaywallsPage() {
  const { data: paywalls, isLoading } = usePaywalls();
  const deletePaywall = useDeletePaywall();
  const [deleteId, setDeleteId] = useState<string | null>(null);

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await deletePaywall.mutateAsync(deleteId);
      toast.success("Paywall deleted");
      setDeleteId(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold tracking-tight">Paywalls</h2>
        <Link href="/admin/paywalls/new">
          <Button>
            <Plus className="mr-2 h-4 w-4" />
            New Paywall
          </Button>
        </Link>
      </div>

      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2">
          {[...Array(2)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : paywalls?.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <p>No paywalls configured yet.</p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {paywalls?.map((pw) => (
            <PaywallCard key={pw.id} paywall={pw} onDelete={setDeleteId} />
          ))}
        </div>
      )}

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
        title="Delete Paywall"
        description={`Are you sure you want to delete "${deleteId}"? Relay policies referencing this paywall will stop working.`}
        onConfirm={handleDelete}
        loading={deletePaywall.isPending}
      />
    </div>
  );
}
