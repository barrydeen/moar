"use client";

import { Button } from "@/components/ui/button";

interface Tab {
  key: string;
  label: string;
}

interface SubTabsProps {
  tabs: Tab[];
  activeTab: string;
  onTabChange: (key: string) => void;
}

export function SubTabs({ tabs, activeTab, onTabChange }: SubTabsProps) {
  return (
    <div className="flex gap-1 border-b pb-1">
      {tabs.map((tab) => (
        <Button
          key={tab.key}
          variant={activeTab === tab.key ? "secondary" : "ghost"}
          size="sm"
          onClick={() => onTabChange(tab.key)}
        >
          {tab.label}
        </Button>
      ))}
    </div>
  );
}
