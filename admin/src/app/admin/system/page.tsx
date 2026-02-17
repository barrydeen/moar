"use client";

import { useState, useEffect, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { restartServer, triggerUpdate, getUpdateStatus, getStatus } from "@/lib/api/status";
import type { UpdateStatus } from "@/lib/api/status";
import { useStatus } from "@/lib/hooks/use-status";
import { Loader2, RotateCcw, Download, CheckCircle2, XCircle } from "lucide-react";
import { toast } from "sonner";

export default function SystemPage() {
  const { data: status } = useStatus();
  const [restarting, setRestarting] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>({ status: "idle" });
  const [polling, setPolling] = useState(false);

  const pollUpdateStatus = useCallback(async () => {
    try {
      const s = await getUpdateStatus();
      setUpdateStatus(s);
      return s;
    } catch {
      return null;
    }
  }, []);

  // Poll update status when an update is in progress
  useEffect(() => {
    if (!polling) return;

    const interval = setInterval(async () => {
      const s = await pollUpdateStatus();
      if (s && (s.status === "complete" || s.status === "error" || s.status === "idle")) {
        setPolling(false);
      }
    }, 2000);

    return () => clearInterval(interval);
  }, [polling, pollUpdateStatus]);

  // Initial fetch of update status
  useEffect(() => {
    pollUpdateStatus().then((s) => {
      if (s && (s.status === "pulling" || s.status === "building")) {
        setPolling(true);
      }
    });
  }, [pollUpdateStatus]);

  async function handleRestart() {
    setRestarting(true);
    try {
      await restartServer();
    } catch {
      // Server may drop connection during restart
    }

    // Poll until server comes back
    const maxAttempts = 30;
    let attempts = 0;

    const poll = () => {
      setTimeout(async () => {
        attempts++;
        try {
          await getStatus();
          window.location.reload();
        } catch {
          if (attempts < maxAttempts) {
            poll();
          } else {
            setRestarting(false);
            toast.error("Server did not come back after restart");
          }
        }
      }, 2000);
    };

    poll();
  }

  async function handleUpdate() {
    try {
      await triggerUpdate();
      setUpdateStatus({ status: "pulling" });
      setPolling(true);
      toast.success("Update started");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to start update");
    }
  }

  const isUpdating = updateStatus.status === "pulling" || updateStatus.status === "building";

  return (
    <div className="space-y-6">
      <h2 className="text-2xl font-bold tracking-tight">System</h2>

      <div className="grid gap-6 md:grid-cols-2">
        {/* Update Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Download className="h-5 w-5" />
              Update MOAR
            </CardTitle>
            <CardDescription>
              Pull the latest version and rebuild containers
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {updateStatus.status !== "idle" && (
              <div className="flex items-center gap-2">
                <StatusBadge status={updateStatus.status} />
                {updateStatus.message && (
                  <span className="text-sm text-muted-foreground truncate">
                    {updateStatus.message}
                  </span>
                )}
              </div>
            )}
            <Button
              onClick={handleUpdate}
              disabled={isUpdating}
            >
              {isUpdating ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  {updateStatus.status === "pulling" ? "Pulling..." : "Building..."}
                </>
              ) : (
                <>
                  <Download className="mr-2 h-4 w-4" />
                  Update Now
                </>
              )}
            </Button>
          </CardContent>
        </Card>

        {/* Restart Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <RotateCcw className="h-5 w-5" />
              Restart Server
            </CardTitle>
            <CardDescription>
              Restart the server to apply configuration changes
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {status?.pending_restart && (
              <p className="text-sm text-yellow-400">
                Configuration changes are pending — a restart is needed.
              </p>
            )}
            <Button
              variant={status?.pending_restart ? "default" : "outline"}
              onClick={handleRestart}
              disabled={restarting}
            >
              {restarting ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Restarting...
                </>
              ) : (
                <>
                  <RotateCcw className="mr-2 h-4 w-4" />
                  Restart Now
                </>
              )}
            </Button>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: UpdateStatus["status"] }) {
  switch (status) {
    case "pulling":
      return (
        <Badge variant="outline" className="text-blue-400 border-blue-400/50">
          <Loader2 className="mr-1 h-3 w-3 animate-spin" />
          Pulling
        </Badge>
      );
    case "building":
      return (
        <Badge variant="outline" className="text-blue-400 border-blue-400/50">
          <Loader2 className="mr-1 h-3 w-3 animate-spin" />
          Building
        </Badge>
      );
    case "complete":
      return (
        <Badge variant="outline" className="text-green-400 border-green-400/50">
          <CheckCircle2 className="mr-1 h-3 w-3" />
          Complete
        </Badge>
      );
    case "error":
      return (
        <Badge variant="outline" className="text-red-400 border-red-400/50">
          <XCircle className="mr-1 h-3 w-3" />
          Error
        </Badge>
      );
    default:
      return null;
  }
}
