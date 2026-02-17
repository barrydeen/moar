"use client";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Globe, Lock, Send, Inbox, MessageCircle, Settings } from "lucide-react";

export type RelayPreset = "public" | "private" | "outbox" | "inbox" | "dm" | "advanced";

interface PresetOption {
  id: RelayPreset;
  name: string;
  description: string;
  icon: React.ElementType;
}

const presets: PresetOption[] = [
  {
    id: "public",
    name: "Public",
    description: "Open relay — anyone can read and write",
    icon: Globe,
  },
  {
    id: "private",
    name: "Private",
    description: "Only allowed pubkeys can read and write",
    icon: Lock,
  },
  {
    id: "outbox",
    name: "Outbox",
    description: "Only allowed pubkeys can write, anyone can read (NIP-65)",
    icon: Send,
  },
  {
    id: "inbox",
    name: "Inbox",
    description: "Anyone can write events tagged to allowed pubkeys, only they can read",
    icon: Inbox,
  },
  {
    id: "dm",
    name: "DM Inbox",
    description: "DM relay — accept kind 1059 events tagged to allowed pubkeys",
    icon: MessageCircle,
  },
  {
    id: "advanced",
    name: "Advanced",
    description: "Full control over all policy settings",
    icon: Settings,
  },
];

interface PresetSelectorProps {
  onSelect: (preset: RelayPreset) => void;
}

export function PresetSelector({ onSelect }: PresetSelectorProps) {
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {presets.map((preset) => (
        <Card
          key={preset.id}
          className="cursor-pointer transition-colors hover:border-primary"
          onClick={() => onSelect(preset.id)}
        >
          <CardHeader className="pb-2">
            <div className="flex items-center gap-2">
              <preset.icon className="h-5 w-5 text-primary" />
              <CardTitle className="text-base">{preset.name}</CardTitle>
            </div>
          </CardHeader>
          <CardContent>
            <CardDescription>{preset.description}</CardDescription>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
