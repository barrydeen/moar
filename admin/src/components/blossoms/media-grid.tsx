"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import { useMedia, useDeleteMedia } from "@/lib/hooks/use-media";
import { formatSize, formatTimestamp } from "@/lib/utils/format";
import type { BlobDescriptor } from "@/lib/types/blossom";
import { Trash2, Copy, ExternalLink } from "lucide-react";
import { toast } from "sonner";

interface MediaGridProps {
  blossomId: string;
}

export function MediaGrid({ blossomId }: MediaGridProps) {
  const { data: media, isLoading } = useMedia(blossomId);
  const deleteMutation = useDeleteMedia(blossomId);
  const [deleteItem, setDeleteItem] = useState<BlobDescriptor | null>(null);

  async function handleDelete() {
    if (!deleteItem) return;
    try {
      await deleteMutation.mutateAsync(deleteItem.sha256);
      toast.success("File deleted");
      setDeleteItem(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Delete failed");
    }
  }

  function copyUrl(url: string) {
    navigator.clipboard.writeText(url);
    toast.success("URL copied");
  }

  if (isLoading) {
    return (
      <div className="grid gap-4 grid-cols-2 sm:grid-cols-3 lg:grid-cols-4">
        {[...Array(8)].map((_, i) => (
          <Skeleton key={i} className="aspect-square" />
        ))}
      </div>
    );
  }

  if (!media?.length) {
    return (
      <div className="text-center py-12 text-muted-foreground">
        <p>No media uploaded yet.</p>
      </div>
    );
  }

  const isImage = (type: string) => type.startsWith("image/");

  return (
    <>
      <div className="grid gap-4 grid-cols-2 sm:grid-cols-3 lg:grid-cols-4">
        {media.map((item) => (
          <div
            key={item.sha256}
            className="group relative border rounded-lg overflow-hidden bg-card"
          >
            {isImage(item.type) ? (
              <img
                src={item.url}
                alt=""
                className="aspect-square object-cover w-full"
                loading="lazy"
              />
            ) : (
              <div className="aspect-square flex items-center justify-center bg-muted">
                <span className="text-xs text-muted-foreground font-mono">
                  {item.type.split("/")[1] || "file"}
                </span>
              </div>
            )}
            <div className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity flex items-end">
              <div className="w-full p-2 space-y-1">
                <p className="text-xs text-white truncate font-mono">
                  {item.sha256.slice(0, 16)}...
                </p>
                <p className="text-xs text-white/70">
                  {formatSize(item.size)}
                </p>
                <div className="flex gap-1">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-white hover:text-white hover:bg-white/20"
                    onClick={() => copyUrl(item.url)}
                  >
                    <Copy className="h-3.5 w-3.5" />
                  </Button>
                  <a href={item.url} target="_blank" rel="noopener noreferrer">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-white hover:text-white hover:bg-white/20"
                    >
                      <ExternalLink className="h-3.5 w-3.5" />
                    </Button>
                  </a>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-red-400 hover:text-red-300 hover:bg-white/20"
                    onClick={() => setDeleteItem(item)}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>

      <ConfirmDialog
        open={!!deleteItem}
        onOpenChange={(open) => !open && setDeleteItem(null)}
        title="Delete File"
        description={`Are you sure you want to delete this file? (${deleteItem?.sha256.slice(0, 16)}...)`}
        onConfirm={handleDelete}
        loading={deleteMutation.isPending}
      />
    </>
  );
}
