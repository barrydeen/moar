"use client";

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import type { WotInfo, WotStatus } from "@/lib/types/wot";
import { truncatePubkey, formatTimestamp } from "@/lib/utils/format";
import { Pencil, Trash2 } from "lucide-react";

function getStatusBadge(status: WotStatus) {
  if (status === "Pending") return <Badge variant="secondary">Pending</Badge>;
  if (status === "Ready") return <Badge variant="success">Ready</Badge>;
  if (typeof status === "object" && "Building" in status) {
    return (
      <Badge variant="warning">
        Building ({status.Building.depth_progress}/{status.Building.total_depth})
      </Badge>
    );
  }
  if (typeof status === "object" && "Error" in status) {
    return <Badge variant="destructive">Error</Badge>;
  }
  return null;
}

function getErrorMessage(status: WotStatus): string | null {
  if (typeof status === "object" && "Error" in status) {
    return status.Error.message;
  }
  return null;
}

interface WotCardProps {
  wot: WotInfo;
  onDelete: (id: string) => void;
}

export function WotCard({ wot, onDelete }: WotCardProps) {
  const errorMsg = getErrorMessage(wot.status);

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <CardTitle className="text-base">{wot.id}</CardTitle>
              {getStatusBadge(wot.status)}
            </div>
            <p className="text-sm text-muted-foreground font-mono">
              Seed: {truncatePubkey(wot.config.seed)}
            </p>
          </div>
          <div className="flex gap-1">
            <Link href={`/admin/wot/${wot.id}/edit`}>
              <Button variant="ghost" size="icon" className="h-8 w-8">
                <Pencil className="h-4 w-4" />
              </Button>
            </Link>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-destructive hover:text-destructive"
              onClick={() => onDelete(wot.id)}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="pt-0 space-y-1">
        <div className="flex gap-4 text-sm text-muted-foreground">
          <span>Depth: {wot.config.depth}</span>
          <span>Pubkeys: {wot.pubkey_count.toLocaleString()}</span>
          <span>Update: every {wot.config.update_interval_hours}h</span>
        </div>
        {wot.last_updated && (
          <p className="text-xs text-muted-foreground">
            Last updated: {formatTimestamp(wot.last_updated)}
          </p>
        )}
        {errorMsg && (
          <p className="text-xs text-destructive">{errorMsg}</p>
        )}
      </CardContent>
    </Card>
  );
}
