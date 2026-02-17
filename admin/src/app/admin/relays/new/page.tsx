"use client";

import { useState } from "react";
import { PresetSelector, type RelayPreset } from "@/components/relays/preset-selector";
import { SimpleRelayForm } from "@/components/relays/simple-relay-form";
import { RelayForm } from "@/components/relays/relay-form";

export default function NewRelayPage() {
  const [preset, setPreset] = useState<RelayPreset | null>(null);

  if (!preset) {
    return (
      <div className="space-y-6">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">New Relay</h2>
          <p className="text-sm text-muted-foreground mt-1">Choose a preset to get started.</p>
        </div>
        <PresetSelector onSelect={setPreset} />
      </div>
    );
  }

  if (preset === "advanced") {
    return (
      <div className="space-y-6">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">New Relay (Advanced)</h2>
        </div>
        <RelayForm />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">
          New {preset.charAt(0).toUpperCase() + preset.slice(1)} Relay
        </h2>
      </div>
      <SimpleRelayForm preset={preset} onBack={() => setPreset(null)} />
    </div>
  );
}
