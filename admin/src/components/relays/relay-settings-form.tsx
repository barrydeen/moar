"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { useUpdateRelay } from "@/lib/hooks/use-relays";
import { relaySettingsSchema, type RelaySettingsData } from "@/lib/utils/validation";
import type { Relay } from "@/lib/types/relay";
import { toast } from "sonner";

interface RelaySettingsFormProps {
  relay: Relay;
}

export function RelaySettingsForm({ relay }: RelaySettingsFormProps) {
  const updateRelay = useUpdateRelay();

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<RelaySettingsData>({
    resolver: zodResolver(relaySettingsSchema),
    defaultValues: {
      name: relay.name,
      description: relay.description || "",
      subdomain: relay.subdomain,
      db_path: relay.db_path,
    },
  });

  async function onSubmit(data: RelaySettingsData) {
    const config = {
      name: data.name,
      description: data.description || undefined,
      subdomain: data.subdomain,
      db_path: data.db_path,
      policy: relay.policy,
      nip11: relay.nip11,
    };

    try {
      await updateRelay.mutateAsync({ id: relay.id, config });
      toast.success("Settings saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Save failed");
    }
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4 max-w-2xl">
      <div className="space-y-2">
        <Label htmlFor="name">Display Name</Label>
        <Input id="name" {...register("name")} placeholder="My Relay" />
        {errors.name && <p className="text-xs text-destructive">{errors.name.message}</p>}
      </div>

      <div className="space-y-2">
        <Label htmlFor="description">Description</Label>
        <Input id="description" {...register("description")} placeholder="Optional description" />
      </div>

      <div className="space-y-2">
        <Label htmlFor="subdomain">Subdomain</Label>
        <Input id="subdomain" {...register("subdomain")} placeholder="relay" />
        {errors.subdomain && (
          <p className="text-xs text-destructive">{errors.subdomain.message}</p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor="db_path">Database Path</Label>
        <Input id="db_path" {...register("db_path")} placeholder="/app/data/relay.db" />
        {errors.db_path && (
          <p className="text-xs text-destructive">{errors.db_path.message}</p>
        )}
      </div>

      <Button type="submit" disabled={updateRelay.isPending}>
        {updateRelay.isPending ? "Saving..." : "Save Settings"}
      </Button>
    </form>
  );
}
