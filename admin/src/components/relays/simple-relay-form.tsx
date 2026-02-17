"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { TagInput } from "@/components/shared/tag-input";
import { useCreateRelay } from "@/lib/hooks/use-relays";
import type { RelayPreset } from "./preset-selector";
import type { RelayConfig, PolicyConfig } from "@/lib/types/relay";
import { toast } from "sonner";

function validatePubkey(value: string): string | null {
  if (!/^[0-9a-fA-F]{64}$/.test(value)) {
    return "Must be a 64-character hex pubkey";
  }
  return null;
}

function buildPolicy(preset: RelayPreset, pubkeys: string[]): PolicyConfig {
  const base: PolicyConfig = {
    write: { require_auth: false },
    read: { require_auth: false },
    events: {},
  };

  switch (preset) {
    case "public":
      return base;
    case "private":
      return {
        ...base,
        write: { require_auth: true, allowed_pubkeys: pubkeys },
        read: { require_auth: true, allowed_pubkeys: pubkeys },
      };
    case "outbox":
      return {
        ...base,
        write: { require_auth: true, allowed_pubkeys: pubkeys },
      };
    case "inbox":
      return {
        ...base,
        write: { require_auth: false, tagged_pubkeys: pubkeys },
        read: { require_auth: true, allowed_pubkeys: pubkeys },
      };
    case "dm":
      return {
        ...base,
        write: { require_auth: false, tagged_pubkeys: pubkeys },
        read: { require_auth: true, allowed_pubkeys: pubkeys },
        events: { allowed_kinds: [1059] },
      };
    default:
      return base;
  }
}

const needsPubkeys: RelayPreset[] = ["private", "outbox", "inbox", "dm"];

interface SimpleRelayFormProps {
  preset: RelayPreset;
  onBack: () => void;
}

export function SimpleRelayForm({ preset, onBack }: SimpleRelayFormProps) {
  const router = useRouter();
  const createRelay = useCreateRelay();
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [subdomain, setSubdomain] = useState("");
  const [pubkeys, setPubkeys] = useState<string[]>([]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();

    if (!id || !name || !subdomain) {
      toast.error("Please fill in all required fields");
      return;
    }

    if (needsPubkeys.includes(preset) && pubkeys.length === 0) {
      toast.error("Add at least one pubkey");
      return;
    }

    const config: RelayConfig = {
      name,
      subdomain,
      db_path: `/app/data/${id}.db`,
      policy: buildPolicy(preset, pubkeys),
    };

    try {
      await createRelay.mutateAsync({ id, config });
      toast.success("Relay created");
      router.push("/admin/relays");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to create relay");
    }
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-6 max-w-xl">
      <div className="space-y-2">
        <Label htmlFor="id">Relay ID</Label>
        <Input
          id="id"
          value={id}
          onChange={(e) => setId(e.target.value)}
          placeholder="my-relay"
        />
        <p className="text-xs text-muted-foreground">
          Unique identifier (alphanumeric, hyphens, underscores)
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor="name">Display Name</Label>
        <Input
          id="name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="My Relay"
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="subdomain">Subdomain</Label>
        <Input
          id="subdomain"
          value={subdomain}
          onChange={(e) => setSubdomain(e.target.value)}
          placeholder="relay"
        />
      </div>

      {needsPubkeys.includes(preset) && (
        <div className="space-y-2">
          <Label>Pubkeys</Label>
          <TagInput
            values={pubkeys}
            onChange={setPubkeys}
            placeholder="Add hex pubkey..."
            validate={validatePubkey}
            truncate
          />
        </div>
      )}

      <div className="flex gap-3">
        <Button type="button" variant="outline" onClick={onBack}>
          Back
        </Button>
        <Button type="submit" disabled={createRelay.isPending}>
          {createRelay.isPending ? "Creating..." : "Create Relay"}
        </Button>
      </div>
    </form>
  );
}
