"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { TagInput } from "@/components/shared/tag-input";
import { useDiscoveryRelays, usePutDiscoveryRelays } from "@/lib/hooks/use-wot";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "sonner";

function validateRelayUrl(value: string): string | null {
  if (!value.startsWith("wss://") && !value.startsWith("ws://")) {
    return "Must start with wss:// or ws://";
  }
  return null;
}

export function DiscoveryRelays() {
  const { data: relays, isLoading } = useDiscoveryRelays();
  const putRelays = usePutDiscoveryRelays();

  async function handleChange(values: string[]) {
    try {
      await putRelays.mutateAsync(values);
      toast.success("Discovery relays updated");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to update");
    }
  }

  if (isLoading) return <Skeleton className="h-32" />;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Discovery Relays</CardTitle>
      </CardHeader>
      <CardContent>
        <TagInput
          values={relays || []}
          onChange={handleChange}
          placeholder="wss://relay.example.com"
          validate={validateRelayUrl}
        />
        <p className="text-xs text-muted-foreground mt-2">
          Relays used to fetch follow lists when building the Web of Trust.
        </p>
      </CardContent>
    </Card>
  );
}
