"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import { Radio, HardDrive, Shield, Settings } from "lucide-react";

const tabs = [
  { href: "/admin/relays", label: "Relays", icon: Radio },
  { href: "/admin/blossoms", label: "Blossom", icon: HardDrive },
  { href: "/admin/wot", label: "Web of Trust", icon: Shield },
  { href: "/admin/system", label: "System", icon: Settings },
];

export function TabNavigation() {
  const pathname = usePathname();

  return (
    <nav className="border-b">
      <div className="container mx-auto px-4">
        <div className="flex gap-1">
          {tabs.map((tab) => {
            const isActive = pathname.startsWith(tab.href);
            return (
              <Link
                key={tab.href}
                href={tab.href}
                className={cn(
                  "flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-colors",
                  isActive
                    ? "border-primary text-primary"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                )}
              >
                <tab.icon className="h-4 w-4" />
                {tab.label}
              </Link>
            );
          })}
        </div>
      </div>
    </nav>
  );
}
