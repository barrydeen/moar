"use client";

import { useStatus } from "@/lib/hooks/use-status";
import { AlertTriangle } from "lucide-react";

export function RestartBanner() {
  const { data } = useStatus();

  if (!data?.pending_restart) return null;

  return (
    <div className="bg-yellow-500/20 border-b border-yellow-500/30 px-4 py-2">
      <div className="container mx-auto flex items-center gap-2 text-sm text-yellow-400">
        <AlertTriangle className="h-4 w-4 shrink-0" />
        <span>
          Configuration changed. Restart the server to apply changes.
        </span>
      </div>
    </div>
  );
}
