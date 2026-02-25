"use client";

import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { CollapsibleSection } from "@/components/ui/collapsible-section";
import { useCreateSync, useUpdateSync } from "@/lib/hooks/use-sync";
import { syncFormSchema, type SyncFormData } from "@/lib/utils/validation";
import type { SyncInfo } from "@/lib/types/sync";
import { toast } from "sonner";

interface SyncFormProps {
  sync?: SyncInfo;
  relayIds: string[];
  wotIds: string[];
}

export function SyncForm({ sync, relayIds, wotIds }: SyncFormProps) {
  const router = useRouter();
  const createSync = useCreateSync();
  const updateSync = useUpdateSync();
  const isEdit = !!sync;

  const getAuthorMode = (): "none" | "static" | "wot" => {
    if (sync?.config.authors_from_wot) return "wot";
    if (sync?.config.authors && sync.config.authors.length > 0) return "static";
    return "none";
  };

  const {
    register,
    handleSubmit,
    watch,
    formState: { errors },
  } = useForm<SyncFormData>({
    resolver: zodResolver(syncFormSchema),
    defaultValues: sync
      ? {
          id: sync.id,
          relay: sync.config.relay,
          remote_relays: sync.config.remote_relays.join("\n"),
          interval_minutes: sync.config.interval_minutes,
          author_mode: getAuthorMode(),
          authors: sync.config.authors?.join("\n") ?? "",
          authors_from_wot: sync.config.authors_from_wot ?? "",
          kinds: sync.config.kinds?.join(", ") ?? "",
          tags_json: sync.config.tags ? JSON.stringify(sync.config.tags, null, 2) : "",
          limit: sync.config.limit,
        }
      : {
          id: "",
          relay: relayIds[0] ?? "",
          remote_relays: "",
          interval_minutes: 60,
          author_mode: "none",
          authors: "",
          authors_from_wot: "",
          kinds: "",
          tags_json: "",
          limit: 500,
        },
  });

  const authorMode = watch("author_mode");

  async function onSubmit(data: SyncFormData) {
    const remote_relays = data.remote_relays
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);

    const authors =
      data.author_mode === "static" && data.authors
        ? data.authors
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean)
        : null;

    const authors_from_wot =
      data.author_mode === "wot" && data.authors_from_wot
        ? data.authors_from_wot
        : null;

    const kinds = data.kinds?.trim()
      ? data.kinds.split(",").map((s) => parseInt(s.trim(), 10)).filter((n) => !isNaN(n))
      : null;

    let tags: Record<string, string[]> | null = null;
    if (data.tags_json?.trim()) {
      try {
        tags = JSON.parse(data.tags_json);
      } catch {
        toast.error("Invalid tags JSON");
        return;
      }
    }

    try {
      if (isEdit) {
        await updateSync.mutateAsync({
          id: data.id,
          relay: data.relay,
          remote_relays,
          interval_minutes: data.interval_minutes,
          authors,
          authors_from_wot,
          kinds,
          tags,
          limit: data.limit,
        });
        toast.success("Sync updated");
      } else {
        await createSync.mutateAsync({
          id: data.id,
          relay: data.relay,
          remote_relays,
          interval_minutes: data.interval_minutes,
          authors,
          authors_from_wot,
          kinds,
          tags,
          limit: data.limit,
        });
        toast.success("Sync created");
      }
      router.push("/admin/sync");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Operation failed");
    }
  }

  const isPending = createSync.isPending || updateSync.isPending;

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4 max-w-2xl mx-auto">
      <CollapsibleSection
        title="Sync Configuration"
        description="Pull events from remote relays into a local relay"
        defaultOpen
      >
        <div className="space-y-2">
          <Label htmlFor="id">Sync ID</Label>
          <Input id="id" {...register("id")} disabled={isEdit} placeholder="inbox-pull" />
          {errors.id && <p className="text-xs text-destructive">{errors.id.message}</p>}
        </div>

        <div className="space-y-2">
          <Label htmlFor="relay">Target Relay</Label>
          <select
            id="relay"
            {...register("relay")}
            className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
          >
            {relayIds.map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
          {errors.relay && <p className="text-xs text-destructive">{errors.relay.message}</p>}
          <p className="text-xs text-muted-foreground">
            Local relay to store pulled events into
          </p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="remote_relays">Remote Relays</Label>
          <Textarea
            id="remote_relays"
            {...register("remote_relays")}
            placeholder={"wss://relay.damus.io\nwss://nos.lol"}
            rows={3}
            className="font-mono text-sm"
          />
          {errors.remote_relays && (
            <p className="text-xs text-destructive">{errors.remote_relays.message}</p>
          )}
          <p className="text-xs text-muted-foreground">One relay URL per line</p>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="interval_minutes">Interval (minutes)</Label>
            <Input
              id="interval_minutes"
              type="number"
              {...register("interval_minutes")}
              min={1}
            />
            {errors.interval_minutes && (
              <p className="text-xs text-destructive">{errors.interval_minutes.message}</p>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="limit">Limit per REQ</Label>
            <Input id="limit" type="number" {...register("limit")} min={1} />
            {errors.limit && <p className="text-xs text-destructive">{errors.limit.message}</p>}
          </div>
        </div>
      </CollapsibleSection>

      <CollapsibleSection
        title="Filters"
        description="Filter which events to pull"
        defaultOpen
      >
        <div className="space-y-2">
          <Label>Author Source</Label>
          <div className="flex gap-4">
            <label className="flex items-center gap-2 text-sm">
              <input type="radio" value="none" {...register("author_mode")} />
              No filter
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="radio" value="static" {...register("author_mode")} />
              Static list
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="radio" value="wot" {...register("author_mode")} />
              From WoT
            </label>
          </div>
        </div>

        {authorMode === "static" && (
          <div className="space-y-2">
            <Label htmlFor="authors">Author Pubkeys</Label>
            <Textarea
              id="authors"
              {...register("authors")}
              placeholder="One hex pubkey per line"
              rows={4}
              className="font-mono text-sm"
            />
            {errors.authors && (
              <p className="text-xs text-destructive">{errors.authors.message}</p>
            )}
          </div>
        )}

        {authorMode === "wot" && (
          <div className="space-y-2">
            <Label htmlFor="authors_from_wot">WoT Set</Label>
            <select
              id="authors_from_wot"
              {...register("authors_from_wot")}
              className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            >
              <option value="">Select a WoT...</option>
              {wotIds.map((id) => (
                <option key={id} value={id}>
                  {id}
                </option>
              ))}
            </select>
            {errors.authors_from_wot && (
              <p className="text-xs text-destructive">{errors.authors_from_wot.message}</p>
            )}
            <p className="text-xs text-muted-foreground">
              Authors will be dynamically resolved from the WoT set
            </p>
          </div>
        )}

        <div className="space-y-2">
          <Label htmlFor="kinds">Kinds</Label>
          <Input
            id="kinds"
            {...register("kinds")}
            placeholder="1, 7, 9735"
          />
          <p className="text-xs text-muted-foreground">
            Comma-separated event kinds (leave empty for all)
          </p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="tags_json">Tag Filters (JSON)</Label>
          <Textarea
            id="tags_json"
            {...register("tags_json")}
            placeholder={'{"p": ["hex_pubkey"]}'}
            rows={3}
            className="font-mono text-sm"
          />
          <p className="text-xs text-muted-foreground">
            Optional JSON object mapping tag names to value arrays
          </p>
        </div>
      </CollapsibleSection>

      <div className="flex gap-3 pt-2">
        <Button type="button" variant="outline" onClick={() => router.push("/admin/sync")}>
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? "Saving..." : isEdit ? "Update Sync" : "Create Sync"}
        </Button>
      </div>
    </form>
  );
}
