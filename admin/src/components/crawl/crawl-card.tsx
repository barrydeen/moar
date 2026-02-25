"use client";

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { CrawlInfo, CrawlStatus } from "@/lib/types/crawl";
import { formatTimestamp } from "@/lib/utils/format";
import {
  MoreHorizontal,
  Pencil,
  Trash2,
  Search,
  ChevronRight,
  Pause,
  Play,
} from "lucide-react";
import { usePauseCrawl, useResumeCrawl } from "@/lib/hooks/use-crawl";
import { toast } from "sonner";

function getStatusBadge(status: CrawlStatus) {
  if (status.state === "Idle") return <Badge variant="secondary">Idle</Badge>;
  if (status.state === "Complete") return <Badge variant="success">Complete</Badge>;
  if (status.state === "Paused") return <Badge variant="warning">Paused</Badge>;
  if (status.state === "Crawling") {
    return (
      <Badge variant="warning">
        Crawling ({status.events_so_far.toLocaleString()} events)
      </Badge>
    );
  }
  if (status.state === "Error") {
    return <Badge variant="destructive">Error</Badge>;
  }
  return null;
}

interface CrawlCardProps {
  crawl: CrawlInfo;
  onDelete: (id: string) => void;
}

export function CrawlCard({ crawl, onDelete }: CrawlCardProps) {
  const pauseCrawl = usePauseCrawl();
  const resumeCrawl = useResumeCrawl();

  const authorCount = crawl.config.authors_from_wot
    ? `WoT: ${crawl.config.authors_from_wot}`
    : crawl.config.authors
      ? `${crawl.config.authors.length} authors`
      : "all authors";

  const kindsSummary = crawl.config.kinds
    ? `kinds: ${crawl.config.kinds.join(", ")}`
    : "all kinds";

  const isPaused = crawl.status.state === "Paused";
  const isCrawling = crawl.status.state === "Crawling";

  async function handlePauseResume(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    try {
      if (isPaused) {
        await resumeCrawl.mutateAsync(crawl.id);
        toast.success("Crawl resumed");
      } else {
        await pauseCrawl.mutateAsync(crawl.id);
        toast.success("Crawl paused");
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed");
    }
  }

  return (
    <Link href={`/admin/crawl/${crawl.id}/edit`} className="block group">
      <Card className="border-l-2 border-l-orange-500/70 transition-all group-hover:border-orange-500/50 group-hover:shadow-md group-hover:-translate-y-0.5">
        <CardHeader className="pb-3">
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-2.5">
              <div className="rounded-md bg-orange-500/10 p-1.5">
                <Search className="h-4 w-4 text-orange-500" />
              </div>
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  <CardTitle className="text-base">{crawl.id}</CardTitle>
                  {getStatusBadge(crawl.status)}
                </div>
                <p className="text-sm text-muted-foreground">
                  Target: <span className="font-mono">{crawl.config.relay}</span>
                </p>
              </div>
            </div>
            <div className="flex items-center gap-1">
              {(isCrawling || isPaused) && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={handlePauseResume}
                  disabled={pauseCrawl.isPending || resumeCrawl.isPending}
                  title={isPaused ? "Resume" : "Pause"}
                >
                  {isPaused ? <Play className="h-4 w-4" /> : <Pause className="h-4 w-4" />}
                </Button>
              )}
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 opacity-0 group-hover:opacity-100 transition-opacity"
                    onClick={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                    }}
                  >
                    <MoreHorizontal className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
                  <DropdownMenuItem asChild>
                    <Link
                      href={`/admin/crawl/${crawl.id}/edit`}
                      className="flex items-center gap-2"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                      Edit
                    </Link>
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    className="text-destructive focus:text-destructive cursor-pointer"
                    onClick={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      onDelete(crawl.id);
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5 mr-2" />
                    Delete
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <ChevronRight className="h-4 w-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
          </div>
        </CardHeader>
        <CardContent className="pt-0 space-y-1">
          <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground">
            <span>{crawl.config.remote_relays.length} remote relay(s)</span>
            <span>{authorCount}</span>
            <span>{kindsSummary}</span>
          </div>
          <div className="flex gap-4 text-sm text-muted-foreground">
            <span>Since: {formatTimestamp(crawl.config.since)}</span>
            <span>Events: {crawl.progress.total_events.toLocaleString()}</span>
          </div>
          {crawl.status.state === "Error" && (
            <p className="text-xs text-destructive">{crawl.status.message}</p>
          )}
        </CardContent>
      </Card>
    </Link>
  );
}
