"use client";

import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { useUploadMedia } from "@/lib/hooks/use-media";
import { Upload } from "lucide-react";
import { toast } from "sonner";

interface MediaUploadProps {
  blossomId: string;
}

export function MediaUpload({ blossomId }: MediaUploadProps) {
  const uploadMutation = useUploadMedia(blossomId);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);

  async function handleFiles(files: FileList | null) {
    if (!files || files.length === 0) return;

    setUploading(true);
    let successCount = 0;
    let errorCount = 0;

    for (const file of Array.from(files)) {
      try {
        await uploadMutation.mutateAsync(file);
        successCount++;
      } catch (err) {
        errorCount++;
        toast.error(`Failed to upload ${file.name}: ${err instanceof Error ? err.message : "Unknown error"}`);
      }
    }

    setUploading(false);
    if (successCount > 0) {
      toast.success(`Uploaded ${successCount} file${successCount > 1 ? "s" : ""}`);
    }
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    handleFiles(e.dataTransfer.files);
  }

  return (
    <div
      className="border-2 border-dashed rounded-lg p-8 text-center transition-colors hover:border-primary/50"
      onDragOver={(e) => e.preventDefault()}
      onDrop={handleDrop}
    >
      <Upload className="h-8 w-8 mx-auto text-muted-foreground mb-3" />
      <p className="text-sm text-muted-foreground mb-3">
        Drag and drop files here, or click to browse
      </p>
      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(e) => handleFiles(e.target.files)}
      />
      <Button
        variant="outline"
        onClick={() => fileInputRef.current?.click()}
        disabled={uploading}
      >
        {uploading ? "Uploading..." : "Browse Files"}
      </Button>
    </div>
  );
}
