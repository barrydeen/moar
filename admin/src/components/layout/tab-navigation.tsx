"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import { LayoutDashboard, Radio, HardDrive, Shield, Zap, Settings } from "lucide-react";

const tabs = [
  { href: "/admin", label: "Dashboard", icon: LayoutDashboard, exact: true },
  { href: "/admin/relays", label: "Relays", icon: Radio },
  { href: "/admin/blossoms", label: "Blossom", icon: HardDrive },
  { href: "/admin/wot", label: "Web of Trust", icon: Shield },
  { href: "/admin/paywalls", label: "Paywalls", icon: Zap },
  { href: "/admin/system", label: "System", icon: Settings },
];

export function TabNavigation() {
  const pathname = usePathname();

  return (
    <nav className="border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 sticky top-0 z-40">
      <div className="container mx-auto px-4">
        <div className="flex gap-1">
          {tabs.map((tab) => {
            const isActive = tab.exact
              ? pathname === tab.href
              : pathname.startsWith(tab.href);
            return (
              <Link
                key={tab.href}
                href={tab.href}
                className={cn(
                  "flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-all",
                  isActive
                    ? "border-primary text-primary"
                    : "border-transparent text-muted-foreground hover:text-foreground hover:border-border"
                )}
              >
                <tab.icon className={cn("h-4 w-4", isActive && "text-primary")} />
                {tab.label}
              </Link>
            );
          })}
        </div>
      </div>
    </nav>
  );
}
