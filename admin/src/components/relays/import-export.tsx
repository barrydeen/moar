"use client";

import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { useImportRelay } from "@/lib/hooks/use-relays";
import { exportRelayUrl } from "@/lib/api/relays";
import { toast } from "sonner";
import { Download, Upload } from "lucide-react";
import type { ImportResult } from "@/lib/types/relay";

interface ImportExportProps {
  relayId: string;
}

export function ImportExport({ relayId }: ImportExportProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const importRelay = useImportRelay();
  const [result, setResult] = useState<ImportResult | null>(null);

  function handleExport() {
    window.location.href = exportRelayUrl(relayId);
  }

  async function handleImport() {
    const file = fileInputRef.current?.files?.[0];
    if (!file) {
      toast.error("Please select a .jsonl file");
      return;
    }

    if (!confirm("This will import events into the relay. Continue?")) {
      return;
    }

    try {
      const res = await importRelay.mutateAsync({ id: relayId, file });
      setResult(res);
      toast.success(`Imported ${res.imported} events`);
      if (fileInputRef.current) {
        fileInputRef.current.value = "";
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Import failed");
    }
  }

  return (
    <div className="space-y-4">
      <Label>Import / Export Events (JSONL)</Label>

      <div className="flex flex-col gap-4 sm:flex-row sm:items-end">
        <Button type="button" variant="outline" size="sm" onClick={handleExport}>
          <Download className="mr-1 h-4 w-4" /> Export JSONL
        </Button>

        <div className="flex items-end gap-2">
          <Input
            ref={fileInputRef}
            type="file"
            accept=".jsonl"
            className="max-w-xs"
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleImport}
            disabled={importRelay.isPending}
          >
            <Upload className="mr-1 h-4 w-4" />
            {importRelay.isPending ? "Importing..." : "Import"}
          </Button>
        </div>
      </div>

      {result && (
        <div className="text-sm text-muted-foreground">
          Imported: {result.imported} &middot; Skipped: {result.skipped} &middot; Errors: {result.errors}
        </div>
      )}
    </div>
  );
}
