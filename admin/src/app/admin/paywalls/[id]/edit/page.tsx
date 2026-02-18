"use client";

import { useParams } from "next/navigation";
import { Skeleton } from "@/components/ui/skeleton";
import { PaywallForm } from "@/components/paywalls/paywall-form";
import { usePaywall } from "@/lib/hooks/use-paywalls";

export default function EditPaywallPage() {
  const params = useParams();
  const id = params.id as string;
  const { data: paywall, isLoading } = usePaywall(id);

  if (isLoading) {
    return <Skeleton className="h-96 max-w-xl" />;
  }

  if (!paywall) {
    return <p className="text-muted-foreground">Paywall not found.</p>;
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">
          Edit Paywall: {paywall.id}
        </h2>
      </div>
      <PaywallForm paywall={paywall} />
    </div>
  );
}
