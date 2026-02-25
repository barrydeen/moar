"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { useUpdateRelay } from "@/lib/hooks/use-relays";
import { useWots } from "@/lib/hooks/use-wot";
import { relaySearchSchema, type RelaySearchData } from "@/lib/utils/validation";
import type { Relay } from "@/lib/types/relay";
import { toast } from "sonner";

interface RelaySearchFormProps {
  relay: Relay;
}

export function RelaySearchForm({ relay }: RelaySearchFormProps) {
  const updateRelay = useUpdateRelay();
  const { data: wots } = useWots();
  const wotIds = wots?.map((w) => w.id) ?? [];

  const {
    register,
    handleSubmit,
    watch,
    formState: { errors },
  } = useForm<RelaySearchData>({
    resolver: zodResolver(relaySearchSchema),
    defaultValues: {
      search: {
        enabled: relay.search?.enabled ?? false,
        index_path: relay.search?.index_path ?? "",
        wot: relay.search?.wot ?? "",
        heap_size_mb: relay.search?.heap_size_mb ?? 50,
        searchable_kinds: relay.search?.searchable_kinds?.join(", ") ?? "0, 1, 30023",
        wot_only: relay.search?.wot_only ?? false,
        min_content_length: relay.search?.min_content_length ?? 10,
      },
    },
  });

  const enabled = watch("search.enabled");

  async function onSubmit(data: RelaySearchData) {
    const s = data.search;

    const searchableKinds = s.searchable_kinds?.trim()
      ? s.searchable_kinds.split(",").map((k) => parseInt(k.trim(), 10)).filter((n) => !isNaN(n))
      : null;

    const search = s.enabled
      ? {
          enabled: true,
          index_path: s.index_path || undefined,
          wot: s.wot || null,
          heap_size_mb: s.heap_size_mb,
          searchable_kinds: searchableKinds,
          wot_only: s.wot_only,
          min_content_length: s.min_content_length,
        }
      : null;

    const config = {
      name: relay.name,
      description: relay.description || undefined,
      subdomain: relay.subdomain,
      db_path: relay.db_path,
      policy: relay.policy,
      nip11: relay.nip11 || undefined,
      search,
    };

    try {
      await updateRelay.mutateAsync({ id: relay.id, config });
      toast.success("Search configuration saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Save failed");
    }
  }

  return (
    <div className="space-y-8 max-w-2xl">
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-8">
        <section className="space-y-4">
          <h3 className="text-lg font-medium">NIP-50 Full-Text Search</h3>
          <p className="text-sm text-muted-foreground">
            Enable Tantivy-based full-text search with WoT-weighted ranking. When enabled,
            this relay will support NIP-50 search queries and rank results using the social graph.
          </p>

          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="search_enabled"
              {...register("search.enabled")}
              className="rounded"
            />
            <Label htmlFor="search_enabled" className="font-normal">
              Enable full-text search (NIP-50)
            </Label>
          </div>
        </section>

        {enabled && (
          <>
            <Separator />

            <section className="space-y-4">
              <h3 className="text-lg font-medium">Index Settings</h3>

              <div className="space-y-2">
                <Label htmlFor="index_path">Index Path</Label>
                <Input
                  id="index_path"
                  {...register("search.index_path")}
                  placeholder={`Default: ${relay.db_path}.idx`}
                  className="font-mono text-sm"
                />
                <p className="text-xs text-muted-foreground">
                  Directory for the Tantivy search index. Leave empty for default.
                </p>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="heap_size_mb">Heap Size (MB)</Label>
                  <Input
                    id="heap_size_mb"
                    type="number"
                    {...register("search.heap_size_mb")}
                    min={1}
                  />
                  {errors.search?.heap_size_mb && (
                    <p className="text-xs text-destructive">{errors.search.heap_size_mb.message}</p>
                  )}
                  <p className="text-xs text-muted-foreground">
                    Memory allocated for the index writer
                  </p>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="min_content_length">Min Content Length</Label>
                  <Input
                    id="min_content_length"
                    type="number"
                    {...register("search.min_content_length")}
                    min={0}
                  />
                  {errors.search?.min_content_length && (
                    <p className="text-xs text-destructive">{errors.search.min_content_length.message}</p>
                  )}
                  <p className="text-xs text-muted-foreground">
                    Skip indexing events shorter than this
                  </p>
                </div>
              </div>

              <div className="space-y-2">
                <Label htmlFor="searchable_kinds">Searchable Kinds</Label>
                <Input
                  id="searchable_kinds"
                  {...register("search.searchable_kinds")}
                  placeholder="0, 1, 30023"
                />
                <p className="text-xs text-muted-foreground">
                  Comma-separated event kinds to index (leave empty for defaults: 0, 1, 30023)
                </p>
              </div>
            </section>

            <Separator />

            <section className="space-y-4">
              <h3 className="text-lg font-medium">WoT Ranking</h3>
              <p className="text-sm text-muted-foreground">
                Use Web of Trust depth as a ranking signal. Events from closer social connections
                rank higher in search results.
              </p>

              <div className="space-y-2">
                <Label htmlFor="wot">WoT Set</Label>
                <select
                  id="wot"
                  {...register("search.wot")}
                  className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                >
                  <option value="">None (no WoT ranking)</option>
                  {wotIds.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
                <p className="text-xs text-muted-foreground">
                  Select a WoT set to enable depth-based ranking boost
                </p>
              </div>

              <div className="flex items-center gap-2">
                <input
                  type="checkbox"
                  id="wot_only"
                  {...register("search.wot_only")}
                  className="rounded"
                />
                <Label htmlFor="wot_only" className="font-normal">
                  WoT-only mode (only index events from WoT members)
                </Label>
              </div>
            </section>
          </>
        )}

        <Button type="submit" disabled={updateRelay.isPending}>
          {updateRelay.isPending ? "Saving..." : "Save Search Config"}
        </Button>
      </form>
    </div>
  );
}
