"use client";

import { Skeleton } from "@/components/ui/skeleton";
import { BlossomForm } from "@/components/blossoms/blossom-form";
import { useBlossom } from "@/lib/hooks/use-blossoms";

export default function EditBlossomPage({
  params,
}: {
  params: { id: string };
}) {
  const { id } = params;
  const { data: blossom, isLoading } = useBlossom(id);

  if (isLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-96" />
      </div>
    );
  }

  if (!blossom) {
    return <p className="text-muted-foreground">Blossom server not found.</p>;
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Edit: {blossom.name}</h2>
      </div>
      <BlossomForm blossom={blossom} />
    </div>
  );
}
