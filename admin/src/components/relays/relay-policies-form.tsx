"use client";

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
import { useUpdateRelay } from "@/lib/hooks/use-relays";
import { useWots } from "@/lib/hooks/use-wot";
import { usePaywalls } from "@/lib/hooks/use-paywalls";
import { relayPoliciesSchema, type RelayPoliciesData } from "@/lib/utils/validation";
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

interface RelayPoliciesFormProps {
  relay: Relay;
}

export function RelayPoliciesForm({ relay }: RelayPoliciesFormProps) {
  const updateRelay = useUpdateRelay();
  const { data: wots } = useWots();
  const { data: paywalls } = usePaywalls();

  const {
    register,
    handleSubmit,
    watch,
    setValue,
  } = useForm<RelayPoliciesData>({
    resolver: zodResolver(relayPoliciesSchema),
    defaultValues: {
      policy: {
        write: {
          require_auth: relay.policy.write.require_auth,
          allowed_pubkeys: relay.policy.write.allowed_pubkeys || [],
          blocked_pubkeys: relay.policy.write.blocked_pubkeys || [],
          tagged_pubkeys: relay.policy.write.tagged_pubkeys || [],
          wot: relay.policy.write.wot || null,
          paywall: relay.policy.write.paywall || null,
        },
        read: {
          require_auth: relay.policy.read.require_auth,
          allowed_pubkeys: relay.policy.read.allowed_pubkeys || [],
          wot: relay.policy.read.wot || null,
          paywall: relay.policy.read.paywall || null,
        },
        events: {
          allowed_kinds: relay.policy.events.allowed_kinds || [],
          blocked_kinds: relay.policy.events.blocked_kinds || [],
          min_pow: relay.policy.events.min_pow ?? null,
          max_content_length: relay.policy.events.max_content_length ?? null,
        },
        rate_limit: relay.policy.rate_limit || null,
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

  async function onSubmit(data: RelayPoliciesData) {
    const config = {
      name: relay.name,
      description: relay.description || undefined,
      subdomain: relay.subdomain,
      db_path: relay.db_path,
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
          paywall: data.policy.write.paywall || undefined,
        },
        read: {
          require_auth: data.policy.read.require_auth,
          allowed_pubkeys: data.policy.read.allowed_pubkeys?.length
            ? data.policy.read.allowed_pubkeys
            : undefined,
          wot: data.policy.read.wot || undefined,
          paywall: data.policy.read.paywall || undefined,
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
      nip11: relay.nip11,
    };

    try {
      await updateRelay.mutateAsync({ id: relay.id, config });
      toast.success("Policies saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Save failed");
    }
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4 max-w-2xl mx-auto">
      {/* Write Policy */}
      <CollapsibleSection title="Write Policy" summary={writeSummary} defaultOpen>
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

        {paywalls && paywalls.length > 0 && (
          <div className="space-y-2">
            <Label>Paywall (write)</Label>
            <Select
              value={watch("policy.write.paywall") || "none"}
              onValueChange={(v) =>
                setValue("policy.write.paywall", v === "none" ? null : v)
              }
            >
              <SelectTrigger>
                <SelectValue placeholder="None" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">None</SelectItem>
                {paywalls.map((pw) => (
                  <SelectItem key={pw.id} value={pw.id}>
                    {pw.id} ({pw.price_sats} sats)
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

        {paywalls && paywalls.length > 0 && (
          <div className="space-y-2">
            <Label>Paywall (read)</Label>
            <Select
              value={watch("policy.read.paywall") || "none"}
              onValueChange={(v) =>
                setValue("policy.read.paywall", v === "none" ? null : v)
              }
            >
              <SelectTrigger>
                <SelectValue placeholder="None" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">None</SelectItem>
                {paywalls.map((pw) => (
                  <SelectItem key={pw.id} value={pw.id}>
                    {pw.id} ({pw.price_sats} sats)
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
      <CollapsibleSection title="Rate Limiting" summary="Configure per-IP limits">
        <div className="grid grid-cols-3 gap-4">
          <div className="space-y-2">
            <Label htmlFor="writes_per_minute">Writes per Minute</Label>
            <Input
              id="writes_per_minute"
              type="number"
              {...register("policy.rate_limit.writes_per_minute")}
              placeholder="Default: 20"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="reads_per_minute">Reads per Minute</Label>
            <Input
              id="reads_per_minute"
              type="number"
              {...register("policy.rate_limit.reads_per_minute")}
              placeholder="Default: 60"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="max_connections">Max Connections</Label>
            <Input
              id="max_connections"
              type="number"
              {...register("policy.rate_limit.max_connections")}
              placeholder="Default: 5"
            />
            <p className="text-xs text-muted-foreground">Per IP address</p>
          </div>
        </div>
      </CollapsibleSection>

      <div className="pt-2">
        <Button type="submit" disabled={updateRelay.isPending}>
          {updateRelay.isPending ? "Saving..." : "Save Policies"}
        </Button>
      </div>
    </form>
  );
}
