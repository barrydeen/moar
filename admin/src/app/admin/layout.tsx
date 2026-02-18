import { Header } from "@/components/layout/header";
import { RestartBanner } from "@/components/layout/restart-banner";
import { TabNavigation } from "@/components/layout/tab-navigation";
import { TooltipProvider } from "@/components/ui/tooltip";

export default function AdminLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <TooltipProvider>
      <div className="min-h-screen flex flex-col bg-muted/30">
        <Header />
        <RestartBanner />
        <TabNavigation />
        <main className="flex-1 container mx-auto px-4 py-6">{children}</main>
      </div>
    </TooltipProvider>
  );
}
