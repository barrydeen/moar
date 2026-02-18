"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { NostrAvatar } from "@/components/shared/nostr-avatar";
import { usePaywallWhitelistProfiles } from "@/lib/hooks/use-paywalls";
import { truncatePubkey, formatTimestamp } from "@/lib/utils/format";
import type { NostrProfile } from "@/lib/types/nostr";
import { ChevronDown, ChevronUp, Users, Clock } from "lucide-react";

const INITIAL_SHOW = 24;

interface PaywallWhitelistProps {
  paywallId: string;
  relays: string[];
}

export function PaywallWhitelist({ paywallId, relays }: PaywallWhitelistProps) {
  const [expanded, setExpanded] = useState(false);
  const { data, isPending, error } = usePaywallWhitelistProfiles(
    paywallId,
    relays
  );

  const title = (
    <h4 className="text-sm font-medium flex items-center gap-2">
      <Users className="h-4 w-4" />
      Whitelisted Users
      {data && (
        <span className="text-muted-foreground">({data.entries.length})</span>
      )}
    </h4>
  );

  if (isPending) {
    return (
      <div className="space-y-3">
        {title}
        <div className="grid gap-2 grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
          {[...Array(12)].map((_, i) => (
            <Skeleton key={i} className="h-16 rounded-lg" />
          ))}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="space-y-3">
        {title}
        <p className="text-sm text-destructive">
          Failed to load whitelist: {error.message}
        </p>
      </div>
    );
  }

  if (!data || data.entries.length === 0) {
    return (
      <div className="space-y-3">
        {title}
        <p className="text-sm text-muted-foreground">
          No users have paid for this paywall yet.
        </p>
      </div>
    );
  }

  const { entries, profiles } = data;
  const shown = expanded ? entries : entries.slice(0, INITIAL_SHOW);
  const hasMore = entries.length > INITIAL_SHOW;

  return (
    <div className="space-y-3">
      {title}
      <div className="grid gap-2 grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
        {shown.map((entry) => {
          const profile = profiles.get(entry.pubkey) ?? {
            pubkey: entry.pubkey,
          };
          return (
            <WhitelistAvatar
              key={entry.pubkey}
              profile={profile}
              expiresAt={entry.expires_at}
            />
          );
        })}
      </div>
      {hasMore && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setExpanded(!expanded)}
          className="w-full"
        >
          {expanded ? (
            <>
              <ChevronUp className="mr-2 h-4 w-4" />
              Show fewer
            </>
          ) : (
            <>
              <ChevronDown className="mr-2 h-4 w-4" />
              Show all {entries.length} users
            </>
          )}
        </Button>
      )}
    </div>
  );
}

function WhitelistAvatar({
  profile,
  expiresAt,
}: {
  profile: NostrProfile;
  expiresAt: number;
}) {
  const now = Math.floor(Date.now() / 1000);
  const daysLeft = Math.ceil((expiresAt - now) / 86400);
  const isExpiringSoon = daysLeft <= 3 && daysLeft > 0;
  const isExpired = daysLeft <= 0;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="flex items-center gap-3 p-2 rounded-lg hover:bg-muted/50 transition-colors min-w-0">
          <NostrAvatar profile={profile} variant="compact" />
          {isExpired ? (
            <Badge
              variant="destructive"
              className="ml-auto text-[10px] px-1.5 shrink-0"
            >
              Expired
            </Badge>
          ) : isExpiringSoon ? (
            <Badge
              variant="warning"
              className="ml-auto text-[10px] px-1.5 shrink-0"
            >
              {daysLeft}d
            </Badge>
          ) : null}
        </div>
      </TooltipTrigger>
      <TooltipContent>
        <div className="space-y-1">
          <p className="font-mono text-xs">{profile.pubkey}</p>
          <p className="text-xs flex items-center gap-1">
            <Clock className="h-3 w-3" />
            Expires: {formatTimestamp(expiresAt)}
          </p>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}
