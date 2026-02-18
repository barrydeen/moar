"use client";

import { useState } from "react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { truncatePubkey } from "@/lib/utils/format";
import type { NostrProfile } from "@/lib/types/nostr";

function pubkeyGradient(pubkey: string): string {
  const h1 = parseInt(pubkey.slice(0, 4), 16) % 360;
  const h2 = (h1 + 120) % 360;
  return `linear-gradient(135deg, hsl(${h1}, 70%, 50%), hsl(${h2}, 70%, 50%))`;
}

function displayName(profile: NostrProfile): string {
  return (
    profile.display_name || profile.name || truncatePubkey(profile.pubkey)
  );
}

interface NostrAvatarProps {
  profile: NostrProfile;
  variant?: "compact" | "full";
}

export function NostrAvatar({ profile, variant = "full" }: NostrAvatarProps) {
  const [imgError, setImgError] = useState(false);
  const size = variant === "compact" ? 32 : 48;
  const name = displayName(profile);

  const avatar = (
    <div
      className="shrink-0 rounded-full overflow-hidden"
      style={{
        width: size,
        height: size,
        background: pubkeyGradient(profile.pubkey),
      }}
    >
      {profile.picture && !imgError && (
        <img
          src={profile.picture}
          alt={name}
          width={size}
          height={size}
          className="w-full h-full object-cover"
          onError={() => setImgError(true)}
        />
      )}
    </div>
  );

  if (variant === "compact") {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="flex items-center gap-2">
            {avatar}
            <span className="text-sm font-medium truncate max-w-[120px]">
              {name}
            </span>
          </div>
        </TooltipTrigger>
        <TooltipContent>
          <span className="font-mono text-xs">{profile.pubkey}</span>
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="flex items-center gap-3 p-2 rounded-lg hover:bg-muted/50 transition-colors min-w-0">
          {avatar}
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium truncate">{name}</p>
            {profile.nip05 && (
              <p className="text-xs text-muted-foreground truncate">
                {profile.nip05}
              </p>
            )}
            <p className="text-xs text-muted-foreground font-mono">
              {truncatePubkey(profile.pubkey)}
            </p>
          </div>
        </div>
      </TooltipTrigger>
      <TooltipContent>
        <span className="font-mono text-xs">{profile.pubkey}</span>
      </TooltipContent>
    </Tooltip>
  );
}
