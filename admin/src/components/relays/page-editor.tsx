"use client";

import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { useRelayPage, usePutRelayPage, useDeleteRelayPage } from "@/lib/hooks/use-relays";
import { toast } from "sonner";
import { Eye, EyeOff, Trash2 } from "lucide-react";

interface PageEditorProps {
  relayId: string;
}

export function PageEditor({ relayId }: PageEditorProps) {
  const { data, isLoading } = useRelayPage(relayId);
  const putPage = usePutRelayPage();
  const deletePage = useDeleteRelayPage();
  const [html, setHtml] = useState("");
  const [showPreview, setShowPreview] = useState(false);

  useEffect(() => {
    if (data?.html) {
      setHtml(data.html);
    }
  }, [data]);

  async function handleSave() {
    try {
      await putPage.mutateAsync({ id: relayId, html });
      toast.success("Page saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to save page");
    }
  }

  async function handleDelete() {
    try {
      await deletePage.mutateAsync(relayId);
      setHtml("");
      toast.success("Page deleted");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to delete page");
    }
  }

  if (isLoading) return <div className="text-sm text-muted-foreground">Loading...</div>;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <Label>Custom Home Page (HTML)</Label>
        <div className="flex gap-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setShowPreview(!showPreview)}
          >
            {showPreview ? (
              <><EyeOff className="mr-1 h-4 w-4" /> Editor</>
            ) : (
              <><Eye className="mr-1 h-4 w-4" /> Preview</>
            )}
          </Button>
          {html && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="text-destructive"
              onClick={handleDelete}
              disabled={deletePage.isPending}
            >
              <Trash2 className="mr-1 h-4 w-4" /> Delete
            </Button>
          )}
        </div>
      </div>

      {showPreview ? (
        <div className="border rounded-md p-4 min-h-[200px]">
          <iframe
            srcDoc={html}
            className="w-full min-h-[200px] border-0"
            sandbox="allow-scripts"
            title="Page preview"
          />
        </div>
      ) : (
        <Textarea
          value={html}
          onChange={(e) => setHtml(e.target.value)}
          placeholder="<html>...</html>"
          className="font-mono text-sm min-h-[200px]"
        />
      )}

      <Button onClick={handleSave} disabled={putPage.isPending} size="sm">
        {putPage.isPending ? "Saving..." : "Save Page"}
      </Button>
    </div>
  );
}
