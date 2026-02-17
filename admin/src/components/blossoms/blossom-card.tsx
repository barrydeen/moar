"use client";

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import type { Blossom } from "@/lib/types/blossom";
import { Pencil, Trash2 } from "lucide-react";

interface BlossomCardProps {
  blossom: Blossom;
  onDelete: (id: string) => void;
  domain?: string;
}

export function BlossomCard({ blossom, onDelete, domain }: BlossomCardProps) {
  const url = domain ? `https://${blossom.subdomain}.${domain}/` : blossom.subdomain;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <CardTitle className="text-base">{blossom.name}</CardTitle>
            <p className="text-sm text-muted-foreground font-mono">{url}</p>
          </div>
          <div className="flex gap-1">
            <Link href={`/admin/blossoms/${blossom.id}/edit`}>
              <Button variant="ghost" size="icon" className="h-8 w-8">
                <Pencil className="h-4 w-4" />
              </Button>
            </Link>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-destructive hover:text-destructive"
              onClick={() => onDelete(blossom.id)}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="pt-0">
        {blossom.description && (
          <p className="text-sm text-muted-foreground mb-2">{blossom.description}</p>
        )}
        <div className="flex flex-wrap gap-1">
          {blossom.policy.upload.allowed_pubkeys?.length ? (
            <Badge variant="secondary" className="text-xs">
              {blossom.policy.upload.allowed_pubkeys.length} uploaders
            </Badge>
          ) : (
            <Badge variant="secondary" className="text-xs">Open uploads</Badge>
          )}
          {blossom.policy.list.require_auth && (
            <Badge variant="secondary" className="text-xs">List auth</Badge>
          )}
          {blossom.policy.max_file_size && (
            <Badge variant="outline" className="text-xs">
              Max {(blossom.policy.max_file_size / (1024 * 1024)).toFixed(0)}MB
            </Badge>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
