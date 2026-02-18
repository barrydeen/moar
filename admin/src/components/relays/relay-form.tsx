"use client";

import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { TagInput } from "@/components/shared/tag-input";
import { CollapsibleSection } from "@/components/ui/collapsible-section";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useCreateRelay, useUpdateRelay } from "@/lib/hooks/use-relays";
import { useWots } from "@/lib/hooks/use-wot";
import { relayFormSchema, type RelayFormData } from "@/lib/utils/validation";
import type { Relay } from "@/lib/types/relay";
import { toast } from "sonner";

function validatePubkey(value: string): string | null {
  if (!/^[0-9a-fA-F]{64}$/.test(value)) return "Must be a 64-character hex pubkey";
  return null;
}

function validateKind(value: string): string | null {
  const n = parseInt(value);
  if (isNaN(n) || n < 0) return "Must be a non-negative integer";
  return null;
}

interface RelayFormProps {
  relay?: Relay;
}

export function RelayForm({ relay }: RelayFormProps) {
  const router = useRouter();
  const createRelay = useCreateRelay();
  const updateRelay = useUpdateRelay();
  const { data: wots } = useWots();
  const isEdit = !!relay;

  const {
    register,
    handleSubmit,
    watch,
    setValue,
    formState: { errors },
  } = useForm<RelayFormData>({
    resolver: zodResolver(relayFormSchema),
    defaultValues: relay
      ? {
          id: relay.id,
          name: relay.name,
          description: relay.description || "",
          subdomain: relay.subdomain,
          db_path: relay.db_path,
          policy: {
            write: {
              require_auth: relay.policy.write.require_auth,
              allowed_pubkeys: relay.policy.write.allowed_pubkeys || [],
              blocked_pubkeys: relay.policy.write.blocked_pubkeys || [],
              tagged_pubkeys: relay.policy.write.tagged_pubkeys || [],
              wot: relay.policy.write.wot || null,
            },
            read: {
              require_auth: relay.policy.read.require_auth,
              allowed_pubkeys: relay.policy.read.allowed_pubkeys || [],
              wot: relay.policy.read.wot || null,
            },
            events: {
              allowed_kinds: relay.policy.events.allowed_kinds || [],
              blocked_kinds: relay.policy.events.blocked_kinds || [],
              min_pow: relay.policy.events.min_pow ?? null,
              max_content_length: relay.policy.events.max_content_length ?? null,
            },
            rate_limit: relay.policy.rate_limit || null,
          },
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
        }
      : {
          id: "",
          name: "",
          description: "",
          subdomain: "",
          db_path: "",
          policy: {
            write: {
              require_auth: false,
              allowed_pubkeys: [],
              blocked_pubkeys: [],
              tagged_pubkeys: [],
              wot: null,
            },
            read: {
              require_auth: false,
              allowed_pubkeys: [],
              wot: null,
            },
            events: {
              allowed_kinds: [],
              blocked_kinds: [],
              min_pow: null,
              max_content_length: null,
            },
            rate_limit: null,
          },
          nip11: {
            icon: "",
            banner: "",
            contact: "",
            terms_of_service: "",
            max_message_length: null,
            max_subscriptions: null,
            max_subid_length: null,
            max_limit: null,
            max_event_tags: null,
            default_limit: null,
            created_at_lower_limit: null,
            created_at_upper_limit: null,
          },
        },
  });

  const writeAuth = watch("policy.write.require_auth");
  const readAuth = watch("policy.read.require_auth");
  const writeAllowed = watch("policy.write.allowed_pubkeys") || [];
  const writeBlocked = watch("policy.write.blocked_pubkeys") || [];
  const writeTagged = watch("policy.write.tagged_pubkeys") || [];
  const readAllowed = watch("policy.read.allowed_pubkeys") || [];
  const allowedKinds = watch("policy.events.allowed_kinds") || [];
  const blockedKinds = watch("policy.events.blocked_kinds") || [];

  // Build summaries for collapsed sections
  const writeSummary = [
    writeAuth && "Auth required",
    writeAllowed.length > 0 && `${writeAllowed.length} allowed`,
    writeBlocked.length > 0 && `${writeBlocked.length} blocked`,
    writeTagged.length > 0 && `${writeTagged.length} tagged`,
  ].filter(Boolean).join(", ") || "Open access";

  const readSummary = [
    readAuth && "Auth required",
    readAllowed.length > 0 && `${readAllowed.length} allowed`,
  ].filter(Boolean).join(", ") || "Open access";

  const eventSummary = [
    allowedKinds.length > 0 && `${allowedKinds.length} allowed kinds`,
    blockedKinds.length > 0 && `${blockedKinds.length} blocked kinds`,
  ].filter(Boolean).join(", ") || "All events allowed";

  async function onSubmit(data: RelayFormData) {
    // Clean up nip11: empty strings become undefined
    const nip11Data = data.nip11;
    const nip11 = nip11Data
      ? {
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
        }
      : undefined;

    // Check if nip11 has any non-undefined values
    const hasNip11 = nip11 && Object.values(nip11).some((v) => v !== undefined);

    // Clean up empty arrays to null
    const config = {
      name: data.name,
      description: data.description || undefined,
      subdomain: data.subdomain,
      db_path: data.db_path,
      policy: {
        write: {
          require_auth: data.policy.write.require_auth,
          allowed_pubkeys: data.policy.write.allowed_pubkeys?.length
            ? data.policy.write.allowed_pubkeys
            : undefined,
          blocked_pubkeys: data.policy.write.blocked_pubkeys?.length
            ? data.policy.write.blocked_pubkeys
            : undefined,
          tagged_pubkeys: data.policy.write.tagged_pubkeys?.length
            ? data.policy.write.tagged_pubkeys
            : undefined,
          wot: data.policy.write.wot || undefined,
        },
        read: {
          require_auth: data.policy.read.require_auth,
          allowed_pubkeys: data.policy.read.allowed_pubkeys?.length
            ? data.policy.read.allowed_pubkeys
            : undefined,
          wot: data.policy.read.wot || undefined,
        },
        events: {
          allowed_kinds: data.policy.events.allowed_kinds?.length
            ? data.policy.events.allowed_kinds
            : undefined,
          blocked_kinds: data.policy.events.blocked_kinds?.length
            ? data.policy.events.blocked_kinds
            : undefined,
          min_pow: data.policy.events.min_pow ?? undefined,
          max_content_length: data.policy.events.max_content_length ?? undefined,
        },
        rate_limit: data.policy.rate_limit ?? undefined,
      },
      nip11: hasNip11 ? nip11 : undefined,
    };

    try {
      if (isEdit) {
        await updateRelay.mutateAsync({ id: data.id, config });
        toast.success("Relay updated");
      } else {
        await createRelay.mutateAsync({ id: data.id, config });
        toast.success("Relay created");
      }
      router.push("/admin/relays");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Operation failed");
    }
  }

  const isPending = createRelay.isPending || updateRelay.isPending;

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4 max-w-2xl mx-auto">
      {/* Basic Info - always open */}
      <CollapsibleSection title="Basic Info" defaultOpen>
        <div className="space-y-2">
          <Label htmlFor="id">Relay ID</Label>
          <Input id="id" {...register("id")} disabled={isEdit} placeholder="my-relay" />
          {errors.id && <p className="text-xs text-destructive">{errors.id.message}</p>}
        </div>

        <div className="space-y-2">
          <Label htmlFor="name">Display Name</Label>
          <Input id="name" {...register("name")} placeholder="My Relay" />
          {errors.name && <p className="text-xs text-destructive">{errors.name.message}</p>}
        </div>

        <div className="space-y-2">
          <Label htmlFor="description">Description</Label>
          <Input id="description" {...register("description")} placeholder="Optional description" />
        </div>

        <div className="grid grid-cols-2 gap-4">
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
        </div>
      </CollapsibleSection>

      {/* Write Policy */}
      <CollapsibleSection title="Write Policy" summary={writeSummary}>
        <div className="flex items-center justify-between rounded-md bg-muted/50 p-3">
          <Label htmlFor="write-auth">Require Authentication</Label>
          <Switch
            id="write-auth"
            checked={writeAuth}
            onCheckedChange={(v) => setValue("policy.write.require_auth", v)}
          />
        </div>

        <div className="space-y-2">
          <Label>Allowed Pubkeys (write)</Label>
          <TagInput
            values={writeAllowed}
            onChange={(v) => setValue("policy.write.allowed_pubkeys", v)}
            placeholder="Add hex pubkey..."
            validate={validatePubkey}
            truncate
          />
        </div>

        <div className="space-y-2">
          <Label>Blocked Pubkeys (write)</Label>
          <TagInput
            values={writeBlocked}
            onChange={(v) => setValue("policy.write.blocked_pubkeys", v)}
            placeholder="Add hex pubkey..."
            validate={validatePubkey}
            truncate
          />
        </div>

        <div className="space-y-2">
          <Label>Tagged Pubkeys (inbox)</Label>
          <TagInput
            values={writeTagged}
            onChange={(v) => setValue("policy.write.tagged_pubkeys", v)}
            placeholder="Add hex pubkey..."
            validate={validatePubkey}
            truncate
          />
          <p className="text-xs text-muted-foreground">
            Events must contain a p-tag for one of these pubkeys
          </p>
        </div>

        {wots && wots.length > 0 && (
          <div className="space-y-2">
            <Label>Web of Trust (write)</Label>
            <Select
              value={watch("policy.write.wot") || "none"}
              onValueChange={(v) =>
                setValue("policy.write.wot", v === "none" ? null : v)
              }
            >
              <SelectTrigger>
                <SelectValue placeholder="None" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">None</SelectItem>
                {wots.map((w) => (
                  <SelectItem key={w.id} value={w.id}>
                    {w.id}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
      </CollapsibleSection>

      {/* Read Policy */}
      <CollapsibleSection title="Read Policy" summary={readSummary}>
        <div className="flex items-center justify-between rounded-md bg-muted/50 p-3">
          <Label htmlFor="read-auth">Require Authentication</Label>
          <Switch
            id="read-auth"
            checked={readAuth}
            onCheckedChange={(v) => setValue("policy.read.require_auth", v)}
          />
        </div>

        <div className="space-y-2">
          <Label>Allowed Pubkeys (read)</Label>
          <TagInput
            values={readAllowed}
            onChange={(v) => setValue("policy.read.allowed_pubkeys", v)}
            placeholder="Add hex pubkey..."
            validate={validatePubkey}
            truncate
          />
        </div>

        {wots && wots.length > 0 && (
          <div className="space-y-2">
            <Label>Web of Trust (read)</Label>
            <Select
              value={watch("policy.read.wot") || "none"}
              onValueChange={(v) =>
                setValue("policy.read.wot", v === "none" ? null : v)
              }
            >
              <SelectTrigger>
                <SelectValue placeholder="None" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">None</SelectItem>
                {wots.map((w) => (
                  <SelectItem key={w.id} value={w.id}>
                    {w.id}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
      </CollapsibleSection>

      {/* Event Policy */}
      <CollapsibleSection title="Event Policy" summary={eventSummary}>
        <div className="space-y-2">
          <Label>Allowed Kinds</Label>
          <TagInput
            values={allowedKinds.map(String)}
            onChange={(v) => setValue("policy.events.allowed_kinds", v.map(Number))}
            placeholder="Add kind number..."
            validate={validateKind}
          />
        </div>

        <div className="space-y-2">
          <Label>Blocked Kinds</Label>
          <TagInput
            values={blockedKinds.map(String)}
            onChange={(v) => setValue("policy.events.blocked_kinds", v.map(Number))}
            placeholder="Add kind number..."
            validate={validateKind}
          />
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="min_pow">Min PoW Bits</Label>
            <Input
              id="min_pow"
              type="number"
              {...register("policy.events.min_pow")}
              placeholder="0"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="max_content_length">Max Content Length</Label>
            <Input
              id="max_content_length"
              type="number"
              {...register("policy.events.max_content_length")}
              placeholder="No limit"
            />
          </div>
        </div>
      </CollapsibleSection>

      {/* Rate Limit */}
      <CollapsibleSection title="Rate Limiting" summary="Configure request limits">
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="writes_per_minute">Writes per Minute</Label>
            <Input
              id="writes_per_minute"
              type="number"
              {...register("policy.rate_limit.writes_per_minute")}
              placeholder="Unlimited"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="reads_per_minute">Reads per Minute</Label>
            <Input
              id="reads_per_minute"
              type="number"
              {...register("policy.rate_limit.reads_per_minute")}
              placeholder="Unlimited"
            />
          </div>
        </div>
      </CollapsibleSection>

      {/* NIP-11 Metadata */}
      <CollapsibleSection
        title="NIP-11 Metadata"
        description="Optional relay information fields exposed via NIP-11."
        summary="Icon, banner, contact info"
      >
        <div className="grid grid-cols-2 gap-4">
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
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="nip11_contact">Contact</Label>
            <Input
              id="nip11_contact"
              {...register("nip11.contact")}
              placeholder="mailto:admin@example.com"
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
        </div>
      </CollapsibleSection>

      {/* Relay Limits */}
      <CollapsibleSection
        title="Relay Limits"
        description="Optional NIP-11 limit values advertised to clients."
        summary="Message length, subscriptions, event constraints"
      >
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
      </CollapsibleSection>

      <div className="flex gap-3 pt-2">
        <Button type="button" variant="outline" onClick={() => router.push("/admin/relays")}>
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? "Saving..." : isEdit ? "Update Relay" : "Create Relay"}
        </Button>
      </div>
    </form>
  );
}
