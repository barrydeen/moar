"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { NostrAvatar } from "@/components/shared/nostr-avatar";
import { useWotFollows } from "@/lib/hooks/use-nostr-profile";
import type { WotInfo } from "@/lib/types/wot";
import { ChevronDown, ChevronUp, Users } from "lucide-react";

const INITIAL_SHOW = 24;

interface WotFollowsProps {
  wot: WotInfo;
  relays: string[];
}

export function WotFollows({ wot, relays }: WotFollowsProps) {
  const [expanded, setExpanded] = useState(false);

  const isReady = wot.status.state === "Ready";
  if (!isReady) return null;

  return <WotFollowsInner wot={wot} relays={relays} />;
}

function WotFollowsInner({
  wot,
  relays,
}: {
  wot: WotInfo;
  relays: string[];
}) {
  const [expanded, setExpanded] = useState(false);
  const { data, isPending, error } = useWotFollows(wot.config.seed, relays);

  if (isPending) {
    return (
      <div className="space-y-3">
        <h4 className="text-sm font-medium flex items-center gap-2">
          <Users className="h-4 w-4" />
          Follows for {wot.id}
        </h4>
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
        <h4 className="text-sm font-medium flex items-center gap-2">
          <Users className="h-4 w-4" />
          Follows for {wot.id}
        </h4>
        <p className="text-sm text-destructive">
          Failed to load follows: {error.message}
        </p>
      </div>
    );
  }

  if (!data || data.contacts.length === 0) {
    return (
      <div className="space-y-3">
        <h4 className="text-sm font-medium flex items-center gap-2">
          <Users className="h-4 w-4" />
          Follows for {wot.id}
        </h4>
        <p className="text-sm text-muted-foreground">
          No follows found on configured discovery relays.
        </p>
      </div>
    );
  }

  const { contacts, profiles } = data;
  const shown = expanded ? contacts : contacts.slice(0, INITIAL_SHOW);
  const hasMore = contacts.length > INITIAL_SHOW;

  return (
    <div className="space-y-3">
      <h4 className="text-sm font-medium flex items-center gap-2">
        <Users className="h-4 w-4" />
        Follows for {wot.id}
        <span className="text-muted-foreground">({contacts.length})</span>
      </h4>
      <div className="grid gap-2 grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
        {shown.map((pubkey) => {
          const profile = profiles.get(pubkey) ?? { pubkey };
          return <NostrAvatar key={pubkey} profile={profile} />;
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
              Show all {contacts.length} follows
            </>
          )}
        </Button>
      )}
    </div>
  );
}
