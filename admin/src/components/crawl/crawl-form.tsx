"use client";

import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { CollapsibleSection } from "@/components/ui/collapsible-section";
import { useCreateCrawl, useUpdateCrawl } from "@/lib/hooks/use-crawl";
import { crawlFormSchema, type CrawlFormData } from "@/lib/utils/validation";
import type { CrawlInfo } from "@/lib/types/crawl";
import { toast } from "sonner";

interface CrawlFormProps {
  crawl?: CrawlInfo;
  relayIds: string[];
  wotIds: string[];
}

export function CrawlForm({ crawl, relayIds, wotIds }: CrawlFormProps) {
  const router = useRouter();
  const createCrawl = useCreateCrawl();
  const updateCrawl = useUpdateCrawl();
  const isEdit = !!crawl;

  const getAuthorMode = (): "none" | "static" | "wot" => {
    if (crawl?.config.authors_from_wot) return "wot";
    if (crawl?.config.authors && crawl.config.authors.length > 0) return "static";
    return "none";
  };

  const formatDate = (ts: number) => {
    const d = new Date(ts * 1000);
    return d.toISOString().slice(0, 10);
  };

  const {
    register,
    handleSubmit,
    watch,
    formState: { errors },
  } = useForm<CrawlFormData>({
    resolver: zodResolver(crawlFormSchema),
    defaultValues: crawl
      ? {
          id: crawl.id,
          relay: crawl.config.relay,
          remote_relays: crawl.config.remote_relays.join("\n"),
          author_mode: getAuthorMode(),
          authors: crawl.config.authors?.join("\n") ?? "",
          authors_from_wot: crawl.config.authors_from_wot ?? "",
          kinds: crawl.config.kinds?.join(", ") ?? "",
          since: formatDate(crawl.config.since),
          until: crawl.config.until ? formatDate(crawl.config.until) : "",
          window_hours: crawl.config.window_hours,
          max_requests_per_second: crawl.config.max_requests_per_second,
          batch_size: crawl.config.batch_size,
          paused: crawl.config.paused,
        }
      : {
          id: "",
          relay: relayIds[0] ?? "",
          remote_relays: "",
          author_mode: "wot",
          authors: "",
          authors_from_wot: wotIds[0] ?? "",
          kinds: "1, 30023",
          since: "2024-01-01",
          until: "",
          window_hours: 24,
          max_requests_per_second: 5,
          batch_size: 100,
          paused: false,
        },
  });

  const authorMode = watch("author_mode");

  async function onSubmit(data: CrawlFormData) {
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

    const since = Math.floor(new Date(data.since).getTime() / 1000);
    const until = data.until?.trim()
      ? Math.floor(new Date(data.until).getTime() / 1000)
      : null;

    try {
      if (isEdit) {
        await updateCrawl.mutateAsync({
          id: data.id,
          relay: data.relay,
          remote_relays,
          authors,
          authors_from_wot,
          kinds,
          since,
          until,
          window_hours: data.window_hours,
          max_requests_per_second: data.max_requests_per_second,
          batch_size: data.batch_size,
          paused: data.paused,
        });
        toast.success("Crawl updated");
      } else {
        await createCrawl.mutateAsync({
          id: data.id,
          relay: data.relay,
          remote_relays,
          authors,
          authors_from_wot,
          kinds,
          since,
          until,
          window_hours: data.window_hours,
          max_requests_per_second: data.max_requests_per_second,
          batch_size: data.batch_size,
          paused: data.paused,
        });
        toast.success("Crawl created");
      }
      router.push("/admin/crawl");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Operation failed");
    }
  }

  const isPending = createCrawl.isPending || updateCrawl.isPending;

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4 max-w-2xl mx-auto">
      <CollapsibleSection
        title="Crawl Configuration"
        description="Exhaustive historical fetch from remote relays"
        defaultOpen
      >
        <div className="space-y-2">
          <Label htmlFor="id">Crawl ID</Label>
          <Input id="id" {...register("id")} disabled={isEdit} placeholder="historical-notes" />
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
            Local relay to store crawled events into
          </p>
        </div>

        <div className="space-y-2">
          <Label htmlFor="remote_relays">Remote Relays</Label>
          <Textarea
            id="remote_relays"
            {...register("remote_relays")}
            placeholder={"wss://relay.damus.io\nwss://nos.lol\nwss://purplepag.es"}
            rows={3}
            className="font-mono text-sm"
          />
          {errors.remote_relays && (
            <p className="text-xs text-destructive">{errors.remote_relays.message}</p>
          )}
          <p className="text-xs text-muted-foreground">One relay URL per line</p>
        </div>
      </CollapsibleSection>

      <CollapsibleSection
        title="Time Range"
        description="Define the historical period to crawl"
        defaultOpen
      >
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="since">Since (start date)</Label>
            <Input id="since" type="date" {...register("since")} />
            {errors.since && <p className="text-xs text-destructive">{errors.since.message}</p>}
          </div>
          <div className="space-y-2">
            <Label htmlFor="until">Until (end date)</Label>
            <Input id="until" type="date" {...register("until")} />
            <p className="text-xs text-muted-foreground">Leave empty for &quot;now&quot;</p>
          </div>
        </div>

        <div className="space-y-2">
          <Label htmlFor="window_hours">Window Size (hours)</Label>
          <Input id="window_hours" type="number" {...register("window_hours")} min={1} />
          {errors.window_hours && (
            <p className="text-xs text-destructive">{errors.window_hours.message}</p>
          )}
          <p className="text-xs text-muted-foreground">
            Time chunk size for each batch of requests (default 24h)
          </p>
        </div>
      </CollapsibleSection>

      <CollapsibleSection
        title="Filters"
        description="Filter which events to crawl"
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
          <Input id="kinds" {...register("kinds")} placeholder="1, 30023" />
          <p className="text-xs text-muted-foreground">
            Comma-separated event kinds (leave empty for all)
          </p>
        </div>
      </CollapsibleSection>

      <CollapsibleSection
        title="Performance"
        description="Rate limiting and batch settings"
      >
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="max_requests_per_second">Max Requests/sec</Label>
            <Input
              id="max_requests_per_second"
              type="number"
              {...register("max_requests_per_second")}
              min={1}
            />
            {errors.max_requests_per_second && (
              <p className="text-xs text-destructive">{errors.max_requests_per_second.message}</p>
            )}
          </div>
          <div className="space-y-2">
            <Label htmlFor="batch_size">Batch Size</Label>
            <Input id="batch_size" type="number" {...register("batch_size")} min={1} />
            {errors.batch_size && (
              <p className="text-xs text-destructive">{errors.batch_size.message}</p>
            )}
            <p className="text-xs text-muted-foreground">Authors per REQ filter</p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <input type="checkbox" id="paused" {...register("paused")} className="rounded" />
          <Label htmlFor="paused" className="font-normal">
            Start paused (don&apos;t begin crawling immediately)
          </Label>
        </div>
      </CollapsibleSection>

      <div className="flex gap-3 pt-2">
        <Button type="button" variant="outline" onClick={() => router.push("/admin/crawl")}>
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? "Saving..." : isEdit ? "Update Crawl" : "Create Crawl"}
        </Button>
      </div>
    </form>
  );
}
