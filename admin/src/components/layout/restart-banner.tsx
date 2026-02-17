"use client";

import { useState } from "react";
import { useStatus } from "@/lib/hooks/use-status";
import { restartServer, getStatus } from "@/lib/api/status";
import { AlertTriangle, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";

export function RestartBanner() {
  const { data } = useStatus();
  const [restarting, setRestarting] = useState(false);

  if (!data?.pending_restart && !restarting) return null;

  async function handleRestart() {
    setRestarting(true);
    try {
      await restartServer();
    } catch {
      // Server may drop the connection during restart — that's expected
    }

    // Poll until server comes back
    const pollInterval = 2000;
    const maxAttempts = 30;
    let attempts = 0;

    const poll = () => {
      setTimeout(async () => {
        attempts++;
        try {
          await getStatus();
          // Server is back — reload the page
          window.location.reload();
        } catch {
          if (attempts < maxAttempts) {
            poll();
          } else {
            setRestarting(false);
          }
        }
      }, pollInterval);
    };

    poll();
  }

  return (
    <div className="bg-yellow-500/20 border-b border-yellow-500/30 px-4 py-2">
      <div className="container mx-auto flex items-center gap-2 text-sm text-yellow-400">
        <AlertTriangle className="h-4 w-4 shrink-0" />
        {restarting ? (
          <>
            <Loader2 className="h-4 w-4 animate-spin" />
            <span>Restarting server...</span>
          </>
        ) : (
          <>
            <span className="flex-1">
              Configuration changed. Restart the server to apply changes.
            </span>
            <Button
              size="sm"
              variant="outline"
              className="border-yellow-500/50 text-yellow-400 hover:bg-yellow-500/20 hover:text-yellow-300"
              onClick={handleRestart}
            >
              Restart Now
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
