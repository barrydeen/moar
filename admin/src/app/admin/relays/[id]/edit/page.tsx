"use client";

import { use, useState } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { SubTabs } from "@/components/shared/sub-tabs";
import { RelaySettingsForm } from "@/components/relays/relay-settings-form";
import { RelayPoliciesForm } from "@/components/relays/relay-policies-form";
import { RelayNip11Form } from "@/components/relays/relay-nip11-form";
import { ImportExport } from "@/components/relays/import-export";
import { useRelay } from "@/lib/hooks/use-relays";

const tabs = [
  { key: "settings", label: "Settings" },
  { key: "policies", label: "Policies" },
  { key: "nip11", label: "NIP-11" },
  { key: "data", label: "Data" },
];

export default function EditRelayPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const { data: relay, isLoading } = useRelay(id);
  const [activeTab, setActiveTab] = useState("settings");

  if (isLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-96" />
      </div>
    );
  }

  if (!relay) {
    return <p className="text-muted-foreground">Relay not found.</p>;
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Edit Relay: {relay.name}</h2>
      </div>
      <SubTabs tabs={tabs} activeTab={activeTab} onTabChange={setActiveTab} />
      {activeTab === "settings" && <RelaySettingsForm relay={relay} />}
      {activeTab === "policies" && <RelayPoliciesForm relay={relay} />}
      {activeTab === "nip11" && <RelayNip11Form relay={relay} relayId={id} />}
      {activeTab === "data" && <ImportExport relayId={id} />}
    </div>
  );
}
