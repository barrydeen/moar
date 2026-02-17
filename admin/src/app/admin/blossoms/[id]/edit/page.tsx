"use client";

import { useState } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { SubTabs } from "@/components/shared/sub-tabs";
import { BlossomForm } from "@/components/blossoms/blossom-form";
import { MediaUpload } from "@/components/blossoms/media-upload";
import { MediaGrid } from "@/components/blossoms/media-grid";
import { useBlossom } from "@/lib/hooks/use-blossoms";

const tabs = [
  { key: "settings", label: "Settings" },
  { key: "media", label: "Media" },
];

export default function EditBlossomPage({
  params,
}: {
  params: { id: string };
}) {
  const { id } = params;
  const { data: blossom, isLoading } = useBlossom(id);
  const [activeTab, setActiveTab] = useState("settings");

  if (isLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-96" />
      </div>
    );
  }

  if (!blossom) {
    return <p className="text-muted-foreground">Blossom server not found.</p>;
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Edit: {blossom.name}</h2>
      </div>
      <SubTabs tabs={tabs} activeTab={activeTab} onTabChange={setActiveTab} />
      {activeTab === "settings" && <BlossomForm blossom={blossom} />}
      {activeTab === "media" && (
        <div className="space-y-6">
          <MediaUpload blossomId={id} />
          <MediaGrid blossomId={id} />
        </div>
      )}
    </div>
  );
}
