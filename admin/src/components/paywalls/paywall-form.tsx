"use client";

import { useRouter } from "next/navigation";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { CollapsibleSection } from "@/components/ui/collapsible-section";
import { useCreatePaywall, useUpdatePaywall } from "@/lib/hooks/use-paywalls";
import type { PaywallInfo } from "@/lib/types/paywall";
import { toast } from "sonner";

const idSchema = z
  .string()
  .min(1, "ID is required")
  .regex(/^[a-zA-Z0-9_-]+$/, "Only alphanumeric, hyphens, and underscores");

const paywallFormSchema = z.object({
  id: idSchema,
  nwc_string: z.string().min(1, "NWC connection string is required"),
  price_sats: z.coerce.number().int().min(1, "Price must be at least 1 sat"),
  period_days: z.coerce.number().int().min(1, "Period must be at least 1 day"),
});

type PaywallFormData = z.infer<typeof paywallFormSchema>;

interface PaywallFormProps {
  paywall?: PaywallInfo;
  nwcString?: string;
}

export function PaywallForm({ paywall, nwcString }: PaywallFormProps) {
  const router = useRouter();
  const createPaywall = useCreatePaywall();
  const updatePaywall = useUpdatePaywall();
  const isEdit = !!paywall;

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<PaywallFormData>({
    resolver: zodResolver(paywallFormSchema),
    defaultValues: paywall
      ? {
          id: paywall.id,
          nwc_string: nwcString || "",
          price_sats: paywall.price_sats,
          period_days: paywall.period_days,
        }
      : {
          id: "",
          nwc_string: "",
          price_sats: 1000,
          period_days: 30,
        },
  });

  async function onSubmit(data: PaywallFormData) {
    try {
      if (isEdit) {
        await updatePaywall.mutateAsync({
          id: data.id,
          nwc_string: data.nwc_string,
          price_sats: data.price_sats,
          period_days: data.period_days,
        });
        toast.success("Paywall updated");
      } else {
        await createPaywall.mutateAsync(data);
        toast.success("Paywall created");
      }
      router.push("/admin/paywalls");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Operation failed");
    }
  }

  const isPending = createPaywall.isPending || updatePaywall.isPending;

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4 max-w-2xl mx-auto">
      <CollapsibleSection title="Paywall Configuration" defaultOpen>
        <div className="space-y-2">
          <Label htmlFor="id">Paywall ID</Label>
          <Input
            id="id"
            {...register("id")}
            disabled={isEdit}
            placeholder="my-paywall"
          />
          {errors.id && (
            <p className="text-xs text-destructive">{errors.id.message}</p>
          )}
        </div>

        <div className="space-y-2">
          <Label htmlFor="nwc_string">NWC Connection String</Label>
          <Input
            id="nwc_string"
            {...register("nwc_string")}
            placeholder="nostr+walletconnect://..."
            className="font-mono text-xs"
            type="password"
          />
          {errors.nwc_string && (
            <p className="text-xs text-destructive">
              {errors.nwc_string.message}
            </p>
          )}
          <p className="text-xs text-muted-foreground">
            Get this from your NWC-compatible wallet (e.g., Alby, Mutiny)
          </p>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="price_sats">Price (sats)</Label>
            <Input
              id="price_sats"
              type="number"
              {...register("price_sats")}
              min={1}
            />
            {errors.price_sats && (
              <p className="text-xs text-destructive">
                {errors.price_sats.message}
              </p>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="period_days">Access Period (days)</Label>
            <Input
              id="period_days"
              type="number"
              {...register("period_days")}
              min={1}
            />
            {errors.period_days && (
              <p className="text-xs text-destructive">
                {errors.period_days.message}
              </p>
            )}
          </div>
        </div>
      </CollapsibleSection>

      <div className="flex gap-3 pt-2">
        <Button
          type="button"
          variant="outline"
          onClick={() => router.push("/admin/paywalls")}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={isPending}>
          {isPending ? "Saving..." : isEdit ? "Update Paywall" : "Create Paywall"}
        </Button>
      </div>
    </form>
  );
}
