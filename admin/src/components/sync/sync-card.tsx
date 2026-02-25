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
import type { SyncInfo, SyncStatus } from "@/lib/types/sync";
import { formatTimestamp } from "@/lib/utils/format";
import {
  MoreHorizontal,
  Pencil,
  Trash2,
  RefreshCw,
  ChevronRight,
  Play,
} from "lucide-react";
import { useTriggerSync } from "@/lib/hooks/use-sync";
import { toast } from "sonner";

function getStatusBadge(status: SyncStatus) {
  if (status.state === "Idle") return <Badge variant="secondary">Idle</Badge>;
  if (status.state === "Ready") return <Badge variant="success">Ready</Badge>;
  if (status.state === "Syncing") {
    return (
      <Badge variant="warning">
        Syncing ({status.events_so_far} events)
      </Badge>
    );
  }
  if (status.state === "Error") {
    return <Badge variant="destructive">Error</Badge>;
  }
  return null;
}

function getErrorMessage(status: SyncStatus): string | null {
  if (status.state === "Error") {
    return status.message;
  }
  return null;
}

interface SyncCardProps {
  sync: SyncInfo;
  onDelete: (id: string) => void;
}

export function SyncCard({ sync, onDelete }: SyncCardProps) {
  const triggerSync = useTriggerSync();
  const errorMsg = getErrorMessage(sync.status);

  const authorCount = sync.config.authors_from_wot
    ? `WoT: ${sync.config.authors_from_wot}`
    : sync.config.authors
      ? `${sync.config.authors.length} authors`
      : "all authors";

  const kindsSummary = sync.config.kinds
    ? `kinds: ${sync.config.kinds.join(", ")}`
    : "all kinds";

  async function handleTrigger(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    try {
      await triggerSync.mutateAsync(sync.id);
      toast.success("Sync triggered");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to trigger sync");
    }
  }

  return (
    <Link href={`/admin/sync/${sync.id}/edit`} className="block group">
      <Card className="border-l-2 border-l-primary/70 transition-all group-hover:border-primary/50 group-hover:shadow-md group-hover:-translate-y-0.5">
        <CardHeader className="pb-3">
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-2.5">
              <div className="rounded-md bg-primary/10 p-1.5">
                <RefreshCw className="h-4 w-4 text-primary" />
              </div>
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  <CardTitle className="text-base">{sync.id}</CardTitle>
                  {getStatusBadge(sync.status)}
                </div>
                <p className="text-sm text-muted-foreground">
                  Target: <span className="font-mono">{sync.config.relay}</span>
                </p>
              </div>
            </div>
            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={handleTrigger}
                disabled={triggerSync.isPending}
                title="Sync Now"
              >
                <Play className="h-4 w-4" />
              </Button>
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
                      href={`/admin/sync/${sync.id}/edit`}
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
                      onDelete(sync.id);
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
            <span>{sync.config.remote_relays.length} remote relay(s)</span>
            <span>{authorCount}</span>
            <span>{kindsSummary}</span>
          </div>
          <div className="flex gap-4 text-sm text-muted-foreground">
            <span>Every {sync.config.interval_minutes}m</span>
            <span>Events pulled: {sync.events_pulled.toLocaleString()}</span>
          </div>
          {sync.last_synced && (
            <p className="text-xs text-muted-foreground">
              Last synced: {formatTimestamp(sync.last_synced)}
            </p>
          )}
          {errorMsg && <p className="text-xs text-destructive">{errorMsg}</p>}
        </CardContent>
      </Card>
    </Link>
  );
}
