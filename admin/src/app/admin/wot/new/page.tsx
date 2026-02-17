"use client";

import { WotForm } from "@/components/wot/wot-form";

export default function NewWotPage() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">New Web of Trust</h2>
      </div>
      <WotForm />
    </div>
  );
}
