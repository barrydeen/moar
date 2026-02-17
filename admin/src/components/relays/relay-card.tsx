"use client";

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { PolicyBadges } from "./policy-badges";
import type { Relay } from "@/lib/types/relay";
import { Pencil, Trash2, Globe } from "lucide-react";

interface RelayCardProps {
  relay: Relay;
  onDelete: (id: string) => void;
  domain?: string;
}

export function RelayCard({ relay, onDelete, domain }: RelayCardProps) {
  const wsUrl = domain ? `wss://${relay.subdomain}.${domain}/` : relay.subdomain;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <CardTitle className="text-base">{relay.name}</CardTitle>
            <p className="text-sm text-muted-foreground font-mono">{wsUrl}</p>
          </div>
          <div className="flex gap-1">
            <Link href={`/admin/relays/${relay.id}/edit`}>
              <Button variant="ghost" size="icon" className="h-8 w-8">
                <Pencil className="h-4 w-4" />
              </Button>
            </Link>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-destructive hover:text-destructive"
              onClick={() => onDelete(relay.id)}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
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
  );
}
