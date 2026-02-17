"use client";

import { Skeleton } from "@/components/ui/skeleton";
import { RelayForm } from "@/components/relays/relay-form";
import { PageEditor } from "@/components/relays/page-editor";
import { Separator } from "@/components/ui/separator";
import { useRelay } from "@/lib/hooks/use-relays";

export default function EditRelayPage({
  params,
}: {
  params: { id: string };
}) {
  const { id } = params;
  const { data: relay, isLoading } = useRelay(id);

  if (isLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-96" />
      </div>
    );
  }

  if (!relay) {
    return <p className="text-muted-foreground">Relay not found.</p>;
  }

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Edit Relay: {relay.name}</h2>
      </div>
      <RelayForm relay={relay} />
      <Separator />
      <PageEditor relayId={id} />
    </div>
  );
}
