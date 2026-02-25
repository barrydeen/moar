"use client";

import { useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { CrawlCard } from "@/components/crawl/crawl-card";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { useCrawls, useDeleteCrawl } from "@/lib/hooks/use-crawl";
import { Plus } from "lucide-react";
import { toast } from "sonner";

export default function CrawlPage() {
  const { data: crawls, isLoading } = useCrawls();
  const deleteCrawl = useDeleteCrawl();
  const [deleteId, setDeleteId] = useState<string | null>(null);

  async function handleDelete() {
    if (!deleteId) return;
    try {
      await deleteCrawl.mutateAsync(deleteId);
      toast.success("Crawl deleted");
      setDeleteId(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete");
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold tracking-tight">Crawl</h2>
        <Link href="/admin/crawl/new">
          <Button>
            <Plus className="mr-2 h-4 w-4" />
            New Crawl
          </Button>
        </Link>
      </div>

      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2">
          {[...Array(2)].map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      ) : crawls?.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <p>No crawl configs yet.</p>
          <p className="text-sm mt-1">
            Create a crawl to exhaustively fetch historical events from remote relays.
          </p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {crawls?.map((crawl) => (
            <CrawlCard key={crawl.id} crawl={crawl} onDelete={setDeleteId} />
          ))}
        </div>
      )}

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(open) => !open && setDeleteId(null)}
        title="Delete Crawl"
        description={`Are you sure you want to delete "${deleteId}"? This will stop the crawl and remove its configuration.`}
        onConfirm={handleDelete}
        loading={deleteCrawl.isPending}
      />
    </div>
  );
}
