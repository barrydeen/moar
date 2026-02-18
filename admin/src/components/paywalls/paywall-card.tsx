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
import type { PaywallInfo } from "@/lib/types/paywall";
import { MoreHorizontal, Pencil, Trash2, Zap, ChevronRight } from "lucide-react";

interface PaywallCardProps {
  paywall: PaywallInfo;
  onDelete: (id: string) => void;
}

export function PaywallCard({ paywall, onDelete }: PaywallCardProps) {
  return (
    <Link href={`/admin/paywalls/${paywall.id}/edit`} className="block group">
      <Card className="border-l-2 border-l-primary/70 transition-all group-hover:border-primary/50 group-hover:shadow-md group-hover:-translate-y-0.5">
        <CardHeader className="pb-3">
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-2.5">
              <div className="rounded-md bg-primary/10 p-1.5">
                <Zap className="h-4 w-4 text-primary" />
              </div>
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  <CardTitle className="text-base">{paywall.id}</CardTitle>
                  <Badge variant="secondary" className="bg-primary/10 text-primary border-0">
                    {paywall.price_sats.toLocaleString()} sats
                  </Badge>
                </div>
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
                    <Link href={`/admin/paywalls/${paywall.id}/edit`} className="flex items-center gap-2">
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
                      onDelete(paywall.id);
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
          <div className="flex gap-4 text-sm text-muted-foreground">
            <span>Period: {paywall.period_days} days</span>
            <span>Whitelisted: {paywall.whitelist_count}</span>
          </div>
        </CardContent>
      </Card>
    </Link>
  );
}
