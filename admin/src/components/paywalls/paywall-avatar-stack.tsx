"use client";

import { useState } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { usePaywallWhitelistProfiles } from "@/lib/hooks/use-paywalls";
import type { NostrProfile } from "@/lib/types/nostr";

function pubkeyGradient(pubkey: string): string {
  const h1 = parseInt(pubkey.slice(0, 4), 16) % 360;
  const h2 = (h1 + 120) % 360;
  return `linear-gradient(135deg, hsl(${h1}, 70%, 50%), hsl(${h2}, 70%, 50%))`;
}

function MiniAvatar({ profile }: { profile: NostrProfile }) {
  const [imgError, setImgError] = useState(false);
  return (
    <div
      className="h-7 w-7 rounded-full ring-2 ring-background overflow-hidden shrink-0"
      style={{ background: pubkeyGradient(profile.pubkey) }}
    >
      {profile.picture && !imgError && (
        <img
          src={profile.picture}
          alt=""
          className="w-full h-full object-cover"
          onError={() => setImgError(true)}
        />
      )}
    </div>
  );
}

interface PaywallAvatarStackProps {
  paywallId: string;
  relays: string[];
}

export function PaywallAvatarStack({
  paywallId,
  relays,
}: PaywallAvatarStackProps) {
  const { data, isPending } = usePaywallWhitelistProfiles(paywallId, relays);

  if (isPending) {
    return (
      <div className="flex items-center mt-2 -space-x-2">
        {[...Array(5)].map((_, i) => (
          <Skeleton
            key={i}
            className="h-7 w-7 rounded-full ring-2 ring-background"
          />
        ))}
      </div>
    );
  }

  if (!data || data.entries.length === 0) return null;

  const MAX_SHOW = 8;
  const shown = data.entries.slice(0, MAX_SHOW);
  const remaining = data.entries.length - MAX_SHOW;

  return (
    <div className="flex items-center mt-2">
      <div className="flex -space-x-2">
        {shown.map((entry) => {
          const profile = data.profiles.get(entry.pubkey) ?? {
            pubkey: entry.pubkey,
          };
          return <MiniAvatar key={entry.pubkey} profile={profile} />;
        })}
      </div>
      {remaining > 0 && (
        <span className="ml-2 text-xs text-muted-foreground">
          +{remaining} more
        </span>
      )}
    </div>
  );
}
