"use client";

import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { MediaGrid } from "@/components/blossoms/media-grid";
import { MediaUpload } from "@/components/blossoms/media-upload";
import { useBlossom } from "@/lib/hooks/use-blossoms";
import { ArrowLeft } from "lucide-react";

export default function MediaPage({
  params,
}: {
  params: { id: string };
}) {
  const { id } = params;
  const { data: blossom } = useBlossom(id);

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Link href="/admin/blossoms">
          <Button variant="ghost" size="icon">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>
        <h2 className="text-2xl font-bold tracking-tight">
          Media: {blossom?.name || id}
        </h2>
      </div>

      <MediaUpload blossomId={id} />
      <Separator />
      <MediaGrid blossomId={id} />
    </div>
  );
}
