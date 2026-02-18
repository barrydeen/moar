"use client";

import { use } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { WotForm } from "@/components/wot/wot-form";
import { useWot } from "@/lib/hooks/use-wot";

export default function EditWotPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const { data: wot, isLoading } = useWot(id);

  if (isLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64" />
      </div>
    );
  }

  if (!wot) {
    return <p className="text-muted-foreground">Web of Trust not found.</p>;
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Edit WoT: {wot.id}</h2>
      </div>
      <WotForm wot={wot} />
    </div>
  );
}
