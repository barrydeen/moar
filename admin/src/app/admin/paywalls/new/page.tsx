"use client";

import { PaywallForm } from "@/components/paywalls/paywall-form";

export default function NewPaywallPage() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">New Paywall</h2>
      </div>
      <PaywallForm />
    </div>
  );
}
