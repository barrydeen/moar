"use client";

import { BlossomForm } from "@/components/blossoms/blossom-form";

export default function NewBlossomPage() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">New Blossom Server</h2>
      </div>
      <BlossomForm />
    </div>
  );
}
