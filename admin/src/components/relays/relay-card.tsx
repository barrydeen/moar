"use client";

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PolicyBadges } from "./policy-badges";
import type { Relay } from "@/lib/types/relay";
import { MoreHorizontal, Pencil, Trash2, Radio, ChevronRight } from "lucide-react";

interface RelayCardProps {
  relay: Relay;
  onDelete: (id: string) => void;
  domain?: string;
}

export function RelayCard({ relay, onDelete, domain }: RelayCardProps) {
  const wsUrl = domain ? `wss://${relay.subdomain}.${domain}/` : relay.subdomain;

  return (
    <Link href={`/admin/relays/${relay.id}/edit`} className="block group">
      <Card className="border-l-2 border-l-primary/70 transition-all group-hover:border-primary/50 group-hover:shadow-md group-hover:-translate-y-0.5">
        <CardHeader className="pb-3">
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-2.5">
              <div className="rounded-md bg-primary/10 p-1.5">
                <Radio className="h-4 w-4 text-primary" />
              </div>
              <div className="space-y-1">
                <CardTitle className="text-base">{relay.name}</CardTitle>
                <p className="text-sm text-muted-foreground font-mono">{wsUrl}</p>
              </div>
            </div>
            <div className="flex items-center gap-1">
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
                    <Link href={`/admin/relays/${relay.id}/edit`} className="flex items-center gap-2">
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
                      onDelete(relay.id);
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
        <CardContent className="pt-0">
          {relay.description && (
            <p className="text-sm text-muted-foreground mb-2">{relay.description}</p>
          )}
          <PolicyBadges policy={relay.policy} />
        </CardContent>
      </Card>
    </Link>
  );
}
