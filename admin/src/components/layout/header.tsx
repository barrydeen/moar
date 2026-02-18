"use client";

import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import { logout } from "@/lib/api/auth";
import { LogOut } from "lucide-react";
import { NostrAvatar } from "@/components/shared/nostr-avatar";
import { useAdminPubkey, useNostrProfile } from "@/lib/hooks/use-nostr-profile";
import { useDiscoveryRelays } from "@/lib/hooks/use-wot";

export function Header() {
  const router = useRouter();
  const { data: pubkey } = useAdminPubkey();
  const { data: relays } = useDiscoveryRelays();
  const { data: profile } = useNostrProfile(pubkey, relays);

  async function handleLogout() {
    await logout();
    router.push("/");
  }

  return (
    <header className="border-b">
      <div className="container mx-auto flex h-14 items-center justify-between px-4">
        <h1 className="text-lg font-bold tracking-tight">MOAR Admin</h1>
        <div className="flex items-center gap-3">
          {profile && <NostrAvatar profile={profile} variant="compact" />}
          <Button variant="ghost" size="sm" onClick={handleLogout}>
            <LogOut className="mr-2 h-4 w-4" />
            Logout
          </Button>
        </div>
      </div>
    </header>
  );
}
