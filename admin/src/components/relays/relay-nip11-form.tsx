"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { PageEditor } from "@/components/relays/page-editor";
import { useUpdateRelay } from "@/lib/hooks/use-relays";
import { relayNip11Schema, type RelayNip11Data } from "@/lib/utils/validation";
import type { Relay } from "@/lib/types/relay";
import { toast } from "sonner";

interface RelayNip11FormProps {
  relay: Relay;
  relayId: string;
}

export function RelayNip11Form({ relay, relayId }: RelayNip11FormProps) {
  const updateRelay = useUpdateRelay();

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<RelayNip11Data>({
    resolver: zodResolver(relayNip11Schema),
    defaultValues: {
      nip11: {
        icon: relay.nip11?.icon || "",
        banner: relay.nip11?.banner || "",
        contact: relay.nip11?.contact || "",
        terms_of_service: relay.nip11?.terms_of_service || "",
        max_message_length: relay.nip11?.max_message_length ?? null,
        max_subscriptions: relay.nip11?.max_subscriptions ?? null,
        max_subid_length: relay.nip11?.max_subid_length ?? null,
        max_limit: relay.nip11?.max_limit ?? null,
        max_event_tags: relay.nip11?.max_event_tags ?? null,
        default_limit: relay.nip11?.default_limit ?? null,
        created_at_lower_limit: relay.nip11?.created_at_lower_limit ?? null,
        created_at_upper_limit: relay.nip11?.created_at_upper_limit ?? null,
      },
    },
  });

  async function onSubmit(data: RelayNip11Data) {
    const nip11Data = data.nip11;
    const nip11 = {
      icon: nip11Data.icon || undefined,
      banner: nip11Data.banner || undefined,
      contact: nip11Data.contact || undefined,
      terms_of_service: nip11Data.terms_of_service || undefined,
      max_message_length: nip11Data.max_message_length ?? undefined,
      max_subscriptions: nip11Data.max_subscriptions ?? undefined,
      max_subid_length: nip11Data.max_subid_length ?? undefined,
      max_limit: nip11Data.max_limit ?? undefined,
      max_event_tags: nip11Data.max_event_tags ?? undefined,
      default_limit: nip11Data.default_limit ?? undefined,
      created_at_lower_limit: nip11Data.created_at_lower_limit ?? undefined,
      created_at_upper_limit: nip11Data.created_at_upper_limit ?? undefined,
    };

    const hasNip11 = Object.values(nip11).some((v) => v !== undefined);

    const config = {
      name: relay.name,
      description: relay.description || undefined,
      subdomain: relay.subdomain,
      db_path: relay.db_path,
      policy: relay.policy,
      nip11: hasNip11 ? nip11 : undefined,
    };

    try {
      await updateRelay.mutateAsync({ id: relay.id, config });
      toast.success("NIP-11 metadata saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Save failed");
    }
  }

  return (
    <div className="space-y-8 max-w-2xl">
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-8">
        {/* NIP-11 Metadata */}
        <section className="space-y-4">
          <h3 className="text-lg font-medium">NIP-11 Metadata</h3>
          <p className="text-sm text-muted-foreground">
            Optional relay information fields exposed via NIP-11.
          </p>

          <div className="space-y-2">
            <Label htmlFor="nip11_icon">Icon URL</Label>
            <Input
              id="nip11_icon"
              {...register("nip11.icon")}
              placeholder="https://example.com/icon.png"
            />
            {errors.nip11?.icon && (
              <p className="text-xs text-destructive">{errors.nip11.icon.message}</p>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="nip11_banner">Banner URL</Label>
            <Input
              id="nip11_banner"
              {...register("nip11.banner")}
              placeholder="https://example.com/banner.png"
            />
            {errors.nip11?.banner && (
              <p className="text-xs text-destructive">{errors.nip11.banner.message}</p>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="nip11_contact">Contact</Label>
            <Input
              id="nip11_contact"
              {...register("nip11.contact")}
              placeholder="mailto:admin@example.com or nostr pubkey"
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="nip11_tos">Terms of Service URL</Label>
            <Input
              id="nip11_tos"
              {...register("nip11.terms_of_service")}
              placeholder="https://example.com/tos"
            />
            {errors.nip11?.terms_of_service && (
              <p className="text-xs text-destructive">{errors.nip11.terms_of_service.message}</p>
            )}
          </div>
        </section>

        <Separator />

        {/* Relay Limits */}
        <section className="space-y-4">
          <h3 className="text-lg font-medium">Relay Limits</h3>
          <p className="text-sm text-muted-foreground">
            Optional NIP-11 limit values advertised to clients.
          </p>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="max_message_length">Max Message Length</Label>
              <Input
                id="max_message_length"
                type="number"
                {...register("nip11.max_message_length")}
                placeholder="No limit"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="max_subscriptions">Max Subscriptions</Label>
              <Input
                id="max_subscriptions"
                type="number"
                {...register("nip11.max_subscriptions")}
                placeholder="No limit"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="max_subid_length">Max Sub ID Length</Label>
              <Input
                id="max_subid_length"
                type="number"
                {...register("nip11.max_subid_length")}
                placeholder="No limit"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="max_limit">Max Limit</Label>
              <Input
                id="max_limit"
                type="number"
                {...register("nip11.max_limit")}
                placeholder="No limit"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="max_event_tags">Max Event Tags</Label>
              <Input
                id="max_event_tags"
                type="number"
                {...register("nip11.max_event_tags")}
                placeholder="No limit"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="default_limit">Default Limit</Label>
              <Input
                id="default_limit"
                type="number"
                {...register("nip11.default_limit")}
                placeholder="No limit"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="created_at_lower_limit">Created At Lower Limit</Label>
              <Input
                id="created_at_lower_limit"
                type="number"
                {...register("nip11.created_at_lower_limit")}
                placeholder="No limit"
              />
              <p className="text-xs text-muted-foreground">Seconds before current time</p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="created_at_upper_limit">Created At Upper Limit</Label>
              <Input
                id="created_at_upper_limit"
                type="number"
                {...register("nip11.created_at_upper_limit")}
                placeholder="No limit"
              />
              <p className="text-xs text-muted-foreground">Seconds after current time</p>
            </div>
          </div>
        </section>

        <Button type="submit" disabled={updateRelay.isPending}>
          {updateRelay.isPending ? "Saving..." : "Save NIP-11"}
        </Button>
      </form>

      <Separator />

      <PageEditor relayId={relayId} />
    </div>
  );
}
