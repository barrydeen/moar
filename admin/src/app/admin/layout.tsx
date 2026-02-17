import { Header } from "@/components/layout/header";
import { RestartBanner } from "@/components/layout/restart-banner";
import { TabNavigation } from "@/components/layout/tab-navigation";

export default function AdminLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <div className="min-h-screen flex flex-col">
      <Header />
      <RestartBanner />
      <TabNavigation />
      <main className="flex-1 container mx-auto px-4 py-6">{children}</main>
    </div>
  );
}
